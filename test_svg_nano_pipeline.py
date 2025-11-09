#!/usr/bin/env python3
"""Test the VTracer→SVG→GPT-5-Nano→JSON pipeline"""

import os
import sys
import base64
import json
import requests
from pathlib import Path

# Use one of the test images
TEST_IMAGE = "/Users/reuben/gauntlet/blueprint/test-data/images/test_blueprint_001.png"
BACKEND_URL = "http://localhost:3000"

def encode_image_to_base64(image_path):
    """Encode image file to base64 string."""
    with open(image_path, 'rb') as f:
        return base64.b64encode(f.read()).decode('utf-8')

def test_upload_image_endpoint():
    """Test the /upload-image endpoint which uses VTracer + GPT-5-Nano AI parsing."""
    print("="*70)
    print("Testing /upload-image endpoint (VTracer → SVG → GPT-5-Nano → Rooms)")
    print("="*70)

    # Check if API key is set
    if not os.environ.get('OPENAI_API_KEY'):
        print("ERROR: OPENAI_API_KEY not set")
        sys.exit(1)

    # Encode image
    print(f"\n1. Loading test image: {TEST_IMAGE}")
    image_b64 = encode_image_to_base64(TEST_IMAGE)
    print(f"   Image encoded: {len(image_b64) / 1024:.1f} KB (base64)")

    # Build request
    payload = {
        "image": image_b64,
        "area_threshold": 100.0,
        "door_threshold": 50.0
    }

    print(f"\n2. Sending request to {BACKEND_URL}/upload-image")
    print(f"   Parameters:")
    print(f"   - area_threshold: {payload['area_threshold']}")
    print(f"   - door_threshold: {payload['door_threshold']}")

    # Send request
    try:
        response = requests.post(
            f"{BACKEND_URL}/upload-image",
            json=payload,
            headers={"Content-Type": "application/json"},
            timeout=60
        )

        if response.status_code == 200:
            result = response.json()

            print("\n3. ✓ SUCCESS!")
            print(f"   Total rooms detected: {result['total_rooms']}")
            print(f"   Lines extracted: {result['lines_extracted']}")
            print(f"   Vectorization complete: {result['vectorization_complete']}")

            print("\n4. Detected Rooms:")
            for i, room in enumerate(result['rooms']):
                print(f"\n   Room {i+1}:")
                print(f"     ID: {room['id']}")
                print(f"     Area: {room['area']:.2f}")
                print(f"     Name hint: {room['name_hint']}")
                print(f"     Bounding box: {room['bounding_box']}")
                print(f"     Points: {len(room['points'])} vertices")

            # Save result to file
            output_file = "test_svg_nano_output.json"
            with open(output_file, 'w') as f:
                json.dump(result, f, indent=2)
            print(f"\n5. Full result saved to: {output_file}")

            return result

        else:
            print(f"\n✗ ERROR: {response.status_code}")
            print(f"Response: {response.text}")
            return None

    except requests.exceptions.ConnectionError:
        print("\n✗ ERROR: Could not connect to backend server")
        print(f"   Is the server running at {BACKEND_URL}?")
        print(f"   Start it with: cd axum-backend && cargo run --release")
        return None
    except Exception as e:
        print(f"\n✗ ERROR: {str(e)}")
        return None

def test_svg_content():
    """Test with the test_apartment.svg file directly."""
    print("\n" + "="*70)
    print("Testing with pre-made SVG (test_apartment.svg)")
    print("="*70)

    svg_path = "/Users/reuben/gauntlet/blueprint/test_apartment.svg"

    print(f"\n1. Loading SVG: {svg_path}")
    with open(svg_path, 'r') as f:
        svg_content = f.read()

    print(f"   SVG size: {len(svg_content)} chars")
    print(f"   Preview:\n{svg_content[:200]}...")

    # For this test, we'd need to add GPT-5-Nano parsing
    # The current /detect/svg endpoint uses geometric parsing only
    print("\n2. Note: /detect/svg uses geometric parsing (not GPT-5-Nano)")
    print("   To test GPT-5-Nano SVG parsing, use /upload-image which:")
    print("   - Converts PNG → SVG via VTracer")
    print("   - Parses SVG → Lines using GPT-5-Nano AI")
    print("   - Detects rooms from parsed lines")

def main():
    print("\n🔍 VTracer→SVG→GPT-5-Nano Pipeline Test\n")

    # Check if test image exists
    if not os.path.exists(TEST_IMAGE):
        print(f"ERROR: Test image not found: {TEST_IMAGE}")
        sys.exit(1)

    # Test the upload-image endpoint (full pipeline)
    result = test_upload_image_endpoint()

    # Show SVG alternative
    test_svg_content()

    print("\n" + "="*70)
    print("Pipeline Flow:")
    print("="*70)
    print("1. PNG Image → VTracer → SVG (vectorization)")
    print("2. SVG → GPT-5-Nano AI Parser → Wall/Door Lines (JSON)")
    print("3. Lines → Graph Builder → Room Detection → Room Polygons")
    print("\nCode locations:")
    print("- VTracer integration: axum-backend/src/image_vectorizer.rs:19-63")
    print("- GPT-5-Nano parser: axum-backend/src/image_vectorizer.rs:392-463")
    print("- Endpoint handler: axum-backend/src/main.rs:544-613")

if __name__ == "__main__":
    main()
