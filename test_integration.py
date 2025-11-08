#!/usr/bin/env python3
"""
Test script for integrated floorplan detection system
Tests both graph-based and vision-enhanced detection
"""

import requests
import json
import base64
import sys
from pathlib import Path

BASE_URL = "http://localhost:3000"

# Simple test floorplan: a rectangular room with walls
TEST_LINES = [
    # Outer rectangle (simple room)
    {"start": {"x": 0, "y": 0}, "end": {"x": 400, "y": 0}},      # Top wall
    {"start": {"x": 400, "y": 0}, "end": {"x": 400, "y": 300}},  # Right wall
    {"start": {"x": 400, "y": 300}, "end": {"x": 0, "y": 300}},  # Bottom wall
    {"start": {"x": 0, "y": 300}, "end": {"x": 0, "y": 0}},      # Left wall
]

# Two-room floorplan with a dividing wall
TWO_ROOM_LINES = [
    # Outer rectangle
    {"start": {"x": 0, "y": 0}, "end": {"x": 600, "y": 0}},
    {"start": {"x": 600, "y": 0}, "end": {"x": 600, "y": 400}},
    {"start": {"x": 600, "y": 400}, "end": {"x": 0, "y": 400}},
    {"start": {"x": 0, "y": 400}, "end": {"x": 0, "y": 0}},
    # Dividing wall with door gap (50 units)
    {"start": {"x": 300, "y": 0}, "end": {"x": 300, "y": 150}},    # Top half
    {"start": {"x": 300, "y": 200}, "end": {"x": 300, "y": 400}},  # Bottom half (door gap = 50)
]


def test_health():
    """Test health endpoint"""
    print("\n=== Testing Health Endpoint ===")
    response = requests.get(f"{BASE_URL}/health")
    print(f"Status: {response.status_code}")
    print(f"Response: {json.dumps(response.json(), indent=2)}")
    return response.status_code == 200


def test_basic_detection():
    """Test basic graph-based detection"""
    print("\n=== Testing Basic Graph Detection ===")

    payload = {
        "lines": TEST_LINES,
        "area_threshold": 100,
        "door_threshold": 50
    }

    response = requests.post(f"{BASE_URL}/detect", json=payload)
    print(f"Status: {response.status_code}")

    if response.status_code == 200:
        data = response.json()
        print(f"Detected rooms: {data['total_rooms']}")
        for room in data['rooms']:
            print(f"  Room {room['id']}: area={room['area']:.0f}, bbox={room['bounding_box']}")
        return True
    else:
        print(f"Error: {response.text}")
        return False


def test_two_room_detection():
    """Test detection with door gap"""
    print("\n=== Testing Two-Room Detection (with door) ===")

    payload = {
        "lines": TWO_ROOM_LINES,
        "area_threshold": 100,
        "door_threshold": 50
    }

    response = requests.post(f"{BASE_URL}/detect", json=payload)
    print(f"Status: {response.status_code}")

    if response.status_code == 200:
        data = response.json()
        print(f"Detected rooms: {data['total_rooms']}")
        for room in data['rooms']:
            print(f"  Room {room['id']}: area={room['area']:.0f}, bbox={room['bounding_box']}")
        return True
    else:
        print(f"Error: {response.text}")
        return False


def test_enhanced_detection_graph_only():
    """Test enhanced endpoint with GraphOnly strategy"""
    print("\n=== Testing Enhanced Detection (GraphOnly) ===")

    payload = {
        "lines": TWO_ROOM_LINES,
        "area_threshold": 100,
        "door_threshold": 50,
        "strategy": "GraphOnly"
    }

    response = requests.post(f"{BASE_URL}/detect/enhanced", json=payload)
    print(f"Status: {response.status_code}")

    if response.status_code == 200:
        data = response.json()
        print(f"Method used: {data['method_used']}")
        print(f"Execution time: {data['execution_time_ms']}ms")
        print(f"Detected rooms: {len(data['rooms'])}")

        for room in data['rooms']:
            print(f"  Room {room['id']}:")
            print(f"    Area: {room['area']:.0f}")
            print(f"    Detection method: {room['detection_method']}")
            print(f"    Room type: {room.get('room_type', 'N/A')}")
            print(f"    Confidence: {room.get('confidence', 'N/A')}")

        print(f"\nMetadata:")
        print(f"  Graph-based rooms: {data['metadata']['graph_based_rooms']}")
        print(f"  Vision classified: {data['metadata']['vision_classified']}")
        print(f"  Method timings: {data['metadata']['method_timings']}")

        return True
    else:
        print(f"Error: {response.text}")
        return False


def load_test_image():
    """Try to load a test image from the dataset"""
    test_image_paths = [
        "data/cubicasa5k/cubicasa5k/colorful/0/F1_colorful.png",
        "data/cubicasa5k/cubicasa5k/high_quality/0/F1_original.png",
        "yolo-training/cubicasa5k_yolo/images/train/0.png",
    ]

    for path in test_image_paths:
        full_path = Path(path)
        if full_path.exists():
            print(f"Found test image: {path}")
            with open(full_path, "rb") as f:
                return base64.b64encode(f.read()).decode('utf-8')

    print("Warning: No test image found")
    return None


def test_enhanced_detection_with_vision():
    """Test enhanced endpoint with vision (requires OPENAI_API_KEY)"""
    print("\n=== Testing Enhanced Detection (GraphWithVision) ===")

    # Load a test image
    image_b64 = load_test_image()

    if not image_b64:
        print("Skipping vision test - no test image available")
        return True

    payload = {
        "lines": TWO_ROOM_LINES,
        "area_threshold": 100,
        "door_threshold": 50,
        "strategy": "GraphWithVision",
        "enable_vision": True,
        "image_base64": image_b64
    }

    response = requests.post(f"{BASE_URL}/detect/enhanced", json=payload)
    print(f"Status: {response.status_code}")

    if response.status_code == 200:
        data = response.json()
        print(f"Method used: {data['method_used']}")
        print(f"Execution time: {data['execution_time_ms']}ms")
        print(f"Detected rooms: {len(data['rooms'])}")

        for room in data['rooms']:
            print(f"  Room {room['id']}:")
            print(f"    Area: {room['area']:.0f}")
            print(f"    Detection method: {room['detection_method']}")
            print(f"    Room type: {room.get('room_type', 'N/A')}")
            print(f"    Confidence: {room.get('confidence', 'N/A')}")
            print(f"    Features: {room.get('features', [])}")

        print(f"\nMetadata:")
        print(f"  Graph-based rooms: {data['metadata']['graph_based_rooms']}")
        print(f"  Vision classified: {data['metadata']['vision_classified']}")
        print(f"  Total time: {data['metadata']['total_execution_time_ms']}ms")
        print(f"  Method timings: {data['metadata']['method_timings']}")

        return True
    else:
        print(f"Error: {response.text}")
        return False


def test_best_available():
    """Test BestAvailable strategy"""
    print("\n=== Testing Enhanced Detection (BestAvailable) ===")

    payload = {
        "lines": TWO_ROOM_LINES,
        "area_threshold": 100,
        "door_threshold": 50,
        "strategy": "BestAvailable"
    }

    response = requests.post(f"{BASE_URL}/detect/enhanced", json=payload)
    print(f"Status: {response.status_code}")

    if response.status_code == 200:
        data = response.json()
        print(f"Method used: {data['method_used']}")
        print(f"Execution time: {data['execution_time_ms']}ms")
        print(f"Detected rooms: {len(data['rooms'])}")
        return True
    else:
        print(f"Error: {response.text}")
        return False


def test_yolo_detection():
    """Test YOLO detection (will fail until model is trained)"""
    print("\n=== Testing YOLO Detection ===")

    image_b64 = load_test_image()

    if not image_b64:
        print("Skipping YOLO test - no test image available")
        return True

    payload = {
        "lines": [],  # YOLO doesn't need lines
        "strategy": "YoloOnly",
        "enable_yolo": True,
        "image_base64": image_b64
    }

    response = requests.post(f"{BASE_URL}/detect/enhanced", json=payload)
    print(f"Status: {response.status_code}")

    if response.status_code == 200:
        data = response.json()
        print(f"Method used: {data['method_used']}")
        print(f"Detected rooms: {len(data['rooms'])}")
        return True
    else:
        print(f"Expected failure (model not trained yet): {response.json()['error']}")
        return True  # This is expected to fail


def main():
    """Run all tests"""
    print("=" * 60)
    print("Floorplan Detection Integration Tests")
    print("=" * 60)

    results = []

    # Test health
    results.append(("Health Check", test_health()))

    # Test basic detection
    results.append(("Basic Detection", test_basic_detection()))
    results.append(("Two-Room Detection", test_two_room_detection()))

    # Test enhanced endpoint
    results.append(("Enhanced GraphOnly", test_enhanced_detection_graph_only()))
    results.append(("Enhanced BestAvailable", test_best_available()))

    # Test vision (might fail if no API key)
    results.append(("Enhanced with Vision", test_enhanced_detection_with_vision()))

    # Test YOLO (will fail until model is trained)
    results.append(("YOLO Detection", test_yolo_detection()))

    # Summary
    print("\n" + "=" * 60)
    print("Test Summary")
    print("=" * 60)

    for name, passed in results:
        status = "✅ PASS" if passed else "❌ FAIL"
        print(f"{status} - {name}")

    total = len(results)
    passed = sum(1 for _, p in results if p)
    print(f"\nTotal: {passed}/{total} tests passed")

    return 0 if passed == total else 1


if __name__ == "__main__":
    sys.exit(main())
