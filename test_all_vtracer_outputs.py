#!/usr/bin/env python3
"""Test all VTracer SVG outputs with GPT-5-Nano to find best configuration"""

import os
import json
import requests
import glob

api_key = os.environ.get('OPENAI_API_KEY')
if not api_key:
    print("ERROR: OPENAI_API_KEY not set")
    exit(1)

system_prompt = """You are an architectural SVG parser. Parse the SVG to extract wall and door segments.

For walls: long straight lines, is_load_bearing: true

For doors: short lines or gaps, is_load_bearing: false, interpolate if needed by connecting endpoints.

Output ONLY JSON: { "walls": [ {"start": {"x": f64, "y": f64}, "end": {"x": f64, "y": f64}, "is_load_bearing": bool } ] }

Use SVG coordinate system. Filter lines <5 units."""

def test_svg_with_gpt5_nano(svg_path):
    """Test an SVG file with GPT-5-Nano"""

    with open(svg_path, 'r') as f:
        svg_content = f.read()

    config_name = os.path.basename(svg_path).replace('output_', '').replace('.svg', '')

    print(f"\n{'='*70}")
    print(f"Testing: {config_name}")
    print(f"{'='*70}")
    print(f"SVG size: {len(svg_content)} bytes ({len(svg_content)/1024:.1f} KB)")

    # Count SVG elements
    path_count = svg_content.count('<path')
    line_count = svg_content.count('<line')
    rect_count = svg_content.count('<rect')
    print(f"SVG elements: {path_count} paths, {line_count} lines, {rect_count} rects")

    payload = {
        "model": "gpt-5-nano",
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": f"SVG content:\n{svg_content}"}
        ],
        "response_format": {"type": "json_object"},
        "max_completion_tokens": 4096
    }

    try:
        response = requests.post(
            "https://api.openai.com/v1/chat/completions",
            headers={
                "Authorization": f"Bearer {api_key}",
                "Content-Type": "application/json"
            },
            json=payload,
            timeout=60
        )

        if response.status_code == 200:
            data = response.json()
            content = data['choices'][0]['message']['content']
            parsed = json.loads(content)

            walls = parsed.get('walls', [])
            usage = data['usage']

            print(f"✓ GPT-5-Nano SUCCESS")
            print(f"  Walls extracted: {len(walls)}")
            print(f"  Tokens: {usage['total_tokens']} ({usage['completion_tokens_details']['reasoning_tokens']} reasoning)")

            if len(walls) > 0:
                print(f"  Sample walls:")
                for i, wall in enumerate(walls[:3]):
                    print(f"    {i+1}. ({wall['start']['x']:.0f}, {wall['start']['y']:.0f}) → ({wall['end']['x']:.0f}, {wall['end']['y']:.0f})")

            return {
                'config': config_name,
                'walls_extracted': len(walls),
                'tokens': usage['total_tokens'],
                'svg_size': len(svg_content),
                'svg_paths': path_count,
                'success': True
            }
        else:
            print(f"✗ API Error: {response.status_code}")
            print(f"  {response.text[:200]}")
            return {'config': config_name, 'success': False, 'error': response.text[:100]}

    except Exception as e:
        print(f"✗ Exception: {str(e)}")
        return {'config': config_name, 'success': False, 'error': str(e)}

def main():
    print("🔍 Testing All VTracer Configurations with GPT-5-Nano")
    print("="*70)

    svg_files = sorted(glob.glob("/tmp/vtracer_outputs/output_*.svg"))

    if not svg_files:
        print("ERROR: No SVG files found in /tmp/vtracer_outputs/")
        exit(1)

    print(f"Found {len(svg_files)} SVG outputs to test\n")

    results = []
    for svg_file in svg_files:
        result = test_svg_with_gpt5_nano(svg_file)
        results.append(result)

    # Summary
    print("\n" + "="*70)
    print("RESULTS SUMMARY")
    print("="*70)

    successful = [r for r in results if r.get('success')]
    successful.sort(key=lambda x: x.get('walls_extracted', 0), reverse=True)

    print(f"\n{'Config':<30} {'Walls':<8} {'SVG Paths':<10} {'Tokens':<10}")
    print("-"*70)

    for r in successful:
        print(f"{r['config']:<30} {r.get('walls_extracted', 0):<8} {r.get('svg_paths', 0):<10} {r.get('tokens', 0):<10}")

    # Best config
    if successful:
        best = successful[0]
        print(f"\n🏆 BEST CONFIGURATION: {best['config']}")
        print(f"   Extracted {best['walls_extracted']} walls")
        print(f"   Used {best['tokens']} tokens")

    # Save results
    with open('/tmp/vtracer_test_results.json', 'w') as f:
        json.dump(results, f, indent=2)
    print(f"\nFull results saved to: /tmp/vtracer_test_results.json")

if __name__ == "__main__":
    main()
