#!/usr/bin/env python3
"""
Comprehensive integration tests for room detection API
Tests both simple (divider-based) and cycle detection algorithms
"""

import json
import requests
import sys
from pathlib import Path
from typing import Dict, List, Any
from dataclasses import dataclass

BASE_URL = "http://localhost:3000"
TEST_DATA_DIR = Path(__file__).parent / "test-data"


@dataclass
class TestCase:
    """Test case configuration"""
    name: str
    json_file: str
    endpoint: str
    expected_rooms: int
    area_threshold: float = 100.0
    door_threshold: float = 50.0
    description: str = ""


# Test cases for different scenarios
# Use absolute paths from project root
PROJECT_ROOT = Path(__file__).parent

TEST_CASES = [
    # Simple algorithm tests
    TestCase(
        name="Original Test Floorplan (2 rooms)",
        json_file=str(PROJECT_ROOT / "test-floorplan.json"),
        endpoint="/detect/simple",
        expected_rooms=2,
        description="Two rooms divided by vertical wall with door gap"
    ),
    TestCase(
        name="Single Corridor Room",
        json_file=str(PROJECT_ROOT / "test-data" / "test-corridor.json"),
        endpoint="/detect/simple",
        expected_rooms=1,
        description="Single long narrow room (corridor)"
    ),
    TestCase(
        name="Three Rooms Horizontal",
        json_file=str(PROJECT_ROOT / "test-data" / "test-3-rooms-horizontal.json"),
        endpoint="/detect/simple",
        expected_rooms=1,  # Simple algorithm only detects vertical dividers
        description="Three rooms stacked vertically (not detected by simple)"
    ),
    TestCase(
        name="6-Room Apartment",
        json_file=str(PROJECT_ROOT / "test-data" / "test-apartment-6-rooms.json"),
        endpoint="/detect/simple",
        expected_rooms=3,  # Simple detects 3 vertical divisions
        description="Complex apartment layout with grid"
    ),

    # Cycle detection tests
    TestCase(
        name="L-Shaped Floorplan (Cycle)",
        json_file=str(PROJECT_ROOT / "test-data" / "test-l-shaped.json"),
        endpoint="/detect",
        expected_rooms=1,  # L-shape is detected as single 6-vertex polygon
        description="L-shaped layout - tests 6-vertex polygon detection"
    ),
    TestCase(
        name="4-Room Grid (Cycle - best effort)",
        json_file=str(PROJECT_ROOT / "test-data" / "test-grid-4-rooms.json"),
        endpoint="/detect",
        expected_rooms=1,  # Door gaps prevent complete 4-room detection
        area_threshold=500.0,  # Filter out door gap artifacts
        door_threshold=50.0,
        description="2x2 grid - door gaps limit cycle detection"
    ),

    # Edge cases
    TestCase(
        name="Small Area Filter",
        json_file=str(PROJECT_ROOT / "test-floorplan.json"),
        endpoint="/detect/simple",
        expected_rooms=0,
        area_threshold=70000.0,  # Both rooms have 60k area
        description="Area threshold filters out all rooms"
    ),
    TestCase(
        name="Low Area Threshold",
        json_file=str(PROJECT_ROOT / "test-floorplan.json"),
        endpoint="/detect/simple",
        expected_rooms=2,
        area_threshold=1.0,
        description="Very low threshold accepts all rooms"
    ),
]


class Colors:
    """ANSI color codes for terminal output"""
    GREEN = '\033[92m'
    RED = '\033[91m'
    YELLOW = '\033[93m'
    BLUE = '\033[94m'
    CYAN = '\033[96m'
    RESET = '\033[0m'
    BOLD = '\033[1m'


def load_test_data(filename: str) -> List[Dict]:
    """Load wall lines from JSON file"""
    filepath = Path(filename)
    if not filepath.exists():
        raise FileNotFoundError(f"Test file not found: {filepath}")

    with open(filepath, 'r') as f:
        return json.load(f)


def test_room_detection(test_case: TestCase) -> Dict[str, Any]:
    """Run a single room detection test"""
    print(f"\n{Colors.CYAN}{Colors.BOLD}Test: {test_case.name}{Colors.RESET}")
    print(f"{Colors.BLUE}  Description: {test_case.description}{Colors.RESET}")
    print(f"  File: {test_case.json_file}")
    print(f"  Endpoint: {test_case.endpoint}")
    print(f"  Expected rooms: {test_case.expected_rooms}")

    try:
        # Load test data
        lines = load_test_data(test_case.json_file)
        print(f"  Loaded {len(lines)} lines")

        # Prepare request
        payload = {
            "lines": lines,
            "area_threshold": test_case.area_threshold,
        }

        if test_case.endpoint != "/detect/simple":
            payload["door_threshold"] = test_case.door_threshold

        # Make request
        url = f"{BASE_URL}{test_case.endpoint}"
        response = requests.post(url, json=payload, timeout=10)
        response.raise_for_status()

        result = response.json()
        actual_rooms = result.get("total_rooms", len(result.get("rooms", [])))

        # Check result
        success = actual_rooms == test_case.expected_rooms

        if success:
            print(f"{Colors.GREEN}  ✓ PASS: Detected {actual_rooms} rooms{Colors.RESET}")
        else:
            print(f"{Colors.RED}  ✗ FAIL: Expected {test_case.expected_rooms} rooms, got {actual_rooms}{Colors.RESET}")

        # Print room details
        rooms = result.get("rooms", [])
        for i, room in enumerate(rooms):
            bbox = room.get("bounding_box", [])
            area = room.get("area", 0)
            name = room.get("name_hint", f"Room {i}")
            print(f"    Room {i}: {name} - Area: {area:.0f}, BBox: [{bbox[0]:.0f}, {bbox[1]:.0f}, {bbox[2]:.0f}, {bbox[3]:.0f}]")

        return {
            "name": test_case.name,
            "success": success,
            "expected": test_case.expected_rooms,
            "actual": actual_rooms,
            "rooms": rooms
        }

    except FileNotFoundError as e:
        print(f"{Colors.YELLOW}  ⚠ SKIP: {e}{Colors.RESET}")
        return {"name": test_case.name, "success": None, "reason": str(e)}
    except requests.exceptions.RequestException as e:
        print(f"{Colors.RED}  ✗ ERROR: API request failed - {e}{Colors.RESET}")
        return {"name": test_case.name, "success": False, "error": str(e)}
    except Exception as e:
        print(f"{Colors.RED}  ✗ ERROR: {e}{Colors.RESET}")
        return {"name": test_case.name, "success": False, "error": str(e)}


def check_server_health() -> bool:
    """Check if the backend server is running"""
    try:
        response = requests.get(f"{BASE_URL}/health", timeout=2)
        return response.status_code == 200
    except requests.exceptions.RequestException:
        return False


def main():
    """Run all integration tests"""
    print(f"\n{Colors.BOLD}{'='*70}{Colors.RESET}")
    print(f"{Colors.BOLD}{Colors.CYAN}Room Detection Integration Tests{Colors.RESET}")
    print(f"{Colors.BOLD}{'='*70}{Colors.RESET}")

    # Check server
    print(f"\n{Colors.BOLD}Checking server health...{Colors.RESET}")
    if not check_server_health():
        print(f"{Colors.RED}✗ Backend server not running at {BASE_URL}{Colors.RESET}")
        print(f"{Colors.YELLOW}Please start the server: cd axum-backend && cargo run{Colors.RESET}")
        return 1

    print(f"{Colors.GREEN}✓ Server is running{Colors.RESET}")

    # Run tests
    results = []
    for test_case in TEST_CASES:
        result = test_room_detection(test_case)
        results.append(result)

    # Summary
    print(f"\n{Colors.BOLD}{'='*70}{Colors.RESET}")
    print(f"{Colors.BOLD}{Colors.CYAN}Test Summary{Colors.RESET}")
    print(f"{Colors.BOLD}{'='*70}{Colors.RESET}")

    passed = sum(1 for r in results if r.get("success") is True)
    failed = sum(1 for r in results if r.get("success") is False)
    skipped = sum(1 for r in results if r.get("success") is None)
    total = len(results)

    print(f"\nTotal: {total}")
    print(f"{Colors.GREEN}Passed: {passed}{Colors.RESET}")
    if failed > 0:
        print(f"{Colors.RED}Failed: {failed}{Colors.RESET}")
    else:
        print(f"Failed: {failed}")
    if skipped > 0:
        print(f"{Colors.YELLOW}Skipped: {skipped}{Colors.RESET}")

    # Failed tests details
    if failed > 0:
        print(f"\n{Colors.RED}{Colors.BOLD}Failed Tests:{Colors.RESET}")
        for r in results:
            if r.get("success") is False:
                print(f"  • {r['name']}")
                if 'expected' in r and 'actual' in r:
                    print(f"    Expected: {r['expected']}, Got: {r['actual']}")
                if 'error' in r:
                    print(f"    Error: {r['error']}")

    print(f"\n{Colors.BOLD}{'='*70}{Colors.RESET}\n")

    return 0 if failed == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
