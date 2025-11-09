#!/usr/bin/env python3
"""Test different VTracer configurations to find optimal settings for blueprints"""

import os
import sys
import base64
import json
import requests
import time
from pathlib import Path

# Test with one of the blueprint images
TEST_IMAGE = "/Users/reuben/gauntlet/blueprint/test-data/images/test_blueprint_001.png"
BACKEND_URL = "http://localhost:3000"

def encode_image_to_base64(image_path):
    """Encode image file to base64 string."""
    with open(image_path, 'rb') as f:
        return base64.b64encode(f.read()).decode('utf-8')

def test_vtracer_direct(image_path, config_name="default"):
    """Test VTracer directly by calling it via Rust"""
    print(f"\n{'='*70}")
    print(f"Testing VTracer Config: {config_name}")
    print(f"{'='*70}")

    # We'll create different config presets and test via the backend
    # by sending requests with different preprocessing parameters

    image_b64 = encode_image_to_base64(image_path)

    payload = {
        "image": image_b64,
        "area_threshold": 100.0,
        "door_threshold": 50.0
    }

    try:
        start = time.time()
        response = requests.post(
            f"{BACKEND_URL}/upload-image",
            json=payload,
            headers={"Content-Type": "application/json"},
            timeout=60
        )
        elapsed = time.time() - start

        if response.status_code == 200:
            result = response.json()
            print(f"✓ SUCCESS in {elapsed:.2f}s")
            print(f"  Lines extracted: {result['lines_extracted']}")
            print(f"  Rooms detected: {result['total_rooms']}")

            if result['total_rooms'] > 0:
                print(f"  Room details:")
                for i, room in enumerate(result['rooms'][:3]):  # Show first 3
                    print(f"    Room {i+1}: area={room['area']:.0f}, points={len(room['points'])}")

            return result
        else:
            print(f"✗ ERROR: {response.status_code}")
            print(f"  {response.text[:200]}")
            return None

    except Exception as e:
        print(f"✗ Exception: {str(e)}")
        return None

def create_vtracer_test_script():
    """Create a standalone Rust script to test different VTracer configs"""

    rust_code = '''
use vtracer::{convert_image_to_svg, Config, ColorMode, Hierarchical};
use visioncortex::PathSimplifyMode;
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <input.png> <output_dir>", args[0]);
        std::process::exit(1);
    }

    let input_path = &args[1];
    let output_dir = &args[2];

    // Test different configurations
    let configs = vec![
        ("default_spline", Config {
            color_mode: ColorMode::Binary,
            hierarchical: Hierarchical::Stacked,
            mode: PathSimplifyMode::Spline,
            filter_speckle: 4,
            color_precision: 6,
            layer_difference: 16,
            corner_threshold: 60,
            length_threshold: 4.0,
            max_iterations: 10,
            splice_threshold: 45,
            path_precision: Some(3),
        }),
        ("polygon", Config {
            color_mode: ColorMode::Binary,
            hierarchical: Hierarchical::Stacked,
            mode: PathSimplifyMode::Polygon,
            filter_speckle: 4,
            color_precision: 6,
            layer_difference: 16,
            corner_threshold: 60,
            length_threshold: 4.0,
            max_iterations: 10,
            splice_threshold: 45,
            path_precision: Some(3),
        }),
        ("web_default_spline", Config {
            color_mode: ColorMode::Binary,
            hierarchical: Hierarchical::Stacked,
            mode: PathSimplifyMode::Spline,
            filter_speckle: 37,  // From web UI screenshot
            color_precision: 6,
            layer_difference: 16,
            corner_threshold: 60,
            length_threshold: 4.0,
            max_iterations: 10,
            splice_threshold: 45,
            path_precision: Some(3),
        }),
        ("aggressive_filtering", Config {
            color_mode: ColorMode::Binary,
            hierarchical: Hierarchical::Stacked,
            mode: PathSimplifyMode::Spline,
            filter_speckle: 10,  // More aggressive speckle filtering
            color_precision: 4,   // Less color precision for grouping
            layer_difference: 8,  // Better edge detection
            corner_threshold: 50, // More corners detected
            length_threshold: 2.0, // Include shorter segments
            max_iterations: 15,
            splice_threshold: 30,
            path_precision: Some(2),
        }),
    ];

    for (name, config) in configs {
        let output_path = format!("{}/output_{}.svg", output_dir, name);
        println!("\\nTesting config: {}", name);

        match convert_image_to_svg(input_path, &output_path, config) {
            Ok(_) => {
                // Count lines in SVG
                if let Ok(svg_content) = std::fs::read_to_string(&output_path) {
                    let line_count = svg_content.matches("<line").count();
                    let path_count = svg_content.matches("<path").count();
                    let rect_count = svg_content.matches("<rect").count();
                    println!("  ✓ Success: {} lines, {} paths, {} rects", line_count, path_count, rect_count);
                    println!("  Output: {}", output_path);
                }
            }
            Err(e) => println!("  ✗ Failed: {}", e),
        }
    }
}
'''

    with open("/Users/reuben/gauntlet/blueprint/test_vtracer_settings/src/main.rs", "w") as f:
        f.write(rust_code)

    print("Created Rust test script at: test_vtracer_settings/src/main.rs")

def main():
    print("🔍 VTracer Configuration Testing")
    print("="*70)

    if not os.path.exists(TEST_IMAGE):
        print(f"ERROR: Test image not found: {TEST_IMAGE}")
        sys.exit(1)

    # Check if backend is running
    try:
        response = requests.get(f"{BACKEND_URL}/health", timeout=5)
        if response.status_code != 200:
            print(f"ERROR: Backend not healthy")
            sys.exit(1)
    except:
        print(f"ERROR: Backend not running at {BACKEND_URL}")
        sys.exit(1)

    print(f"\nTest image: {TEST_IMAGE}")
    print(f"Backend: {BACKEND_URL}")

    # Test current configuration
    result = test_vtracer_direct(TEST_IMAGE, "current_config")

    print("\n" + "="*70)
    print("Next Steps:")
    print("="*70)
    print("1. Create dedicated Rust test binary to try different VTracer configs")
    print("2. Compare output SVGs visually and count extracted elements")
    print("3. Update backend config with best settings")
    print("\nSuggested config variations to test:")
    print("  - filter_speckle: 4, 10, 20, 37 (web default)")
    print("  - mode: Spline vs Polygon")
    print("  - corner_threshold: 30, 50, 60")
    print("  - length_threshold: 2.0, 4.0, 8.0")

if __name__ == "__main__":
    main()
