#!/usr/bin/env python3
"""
Test vision-enhanced detection with a real floorplan image
"""

import requests
import json
import base64
import sys
import os

BASE_URL = "http://localhost:3000"
TEST_IMAGE = "/Users/reuben/gauntlet/blueprint/yolo-training/cubicasa5k_yolo/images/test/000387.png"

# Simple test floorplan lines (can be empty since vision works without them)
TEST_LINES = [
    {"start": {"x": 0, "y": 0}, "end": {"x": 400, "y": 0}},
    {"start": {"x": 400, "y": 0}, "end": {"x": 400, "y": 300}},
    {"start": {"x": 400, "y": 300}, "end": {"x": 0, "y": 300}},
    {"start": {"x": 0, "y": 300}, "end": {"x": 0, "y": 0}},
]


def test_vision_detection():
    """Test vision-enhanced detection"""

    # Check for API key
    if not os.environ.get("OPENAI_API_KEY"):
        print("❌ OPENAI_API_KEY not set")
        print("To test vision classification, export your API key:")
        print("  export OPENAI_API_KEY='sk-...'")
        return False

    # Load and encode image
    print(f"Loading test image: {TEST_IMAGE}")
    with open(TEST_IMAGE, "rb") as f:
        image_data = f.read()
        image_b64 = base64.b64encode(image_data).decode('utf-8')

    print(f"Image size: {len(image_data) / 1024:.1f} KB")
    print(f"Base64 size: {len(image_b64) / 1024:.1f} KB\n")

    # Test GraphWithVision strategy
    print("=== Testing GraphWithVision Strategy ===")

    payload = {
        "lines": TEST_LINES,
        "image_base64": image_b64,
        "strategy": "GraphWithVision",
        "enable_vision": True,
        "area_threshold": 100,
        "door_threshold": 50
    }

    print("Sending request to /detect/enhanced...")
    print("(This will take 2-5 seconds for GPT-5 Vision API...)\n")

    response = requests.post(f"{BASE_URL}/detect/enhanced", json=payload)

    if response.status_code != 200:
        print(f"❌ Error {response.status_code}: {response.text}")
        return False

    data = response.json()

    print(f"✅ Success!")
    print(f"\nMethod used: {data['method_used']}")
    print(f"Total execution time: {data['execution_time_ms']}ms")
    print(f"Detected {len(data['rooms'])} rooms\n")

    # Show room details
    for room in data['rooms']:
        print(f"Room {room['id']}:")
        print(f"  Type: {room.get('room_type', 'Unknown')}")
        print(f"  Confidence: {room.get('confidence', 'N/A')}")
        print(f"  Area: {room['area']:.0f}")
        print(f"  Features: {', '.join(room.get('features', []))}")
        print(f"  Detection method: {room['detection_method']}")
        print()

    # Show metadata
    print("Metadata:")
    print(f"  Graph-based rooms: {data['metadata']['graph_based_rooms']}")
    print(f"  Vision classified: {data['metadata']['vision_classified']}")
    print(f"  YOLO detected: {data['metadata']['yolo_detected']}")
    print(f"\nMethod timings:")
    for method, time_ms in data['metadata']['method_timings']:
        print(f"  {method}: {time_ms}ms")

    return True


def main():
    print("=" * 60)
    print("Vision-Enhanced Floorplan Detection Test")
    print("=" * 60)
    print()

    success = test_vision_detection()

    if success:
        print("\n✅ Vision test completed successfully!")
        return 0
    else:
        print("\n❌ Vision test failed")
        return 1


if __name__ == "__main__":
    sys.exit(main())
