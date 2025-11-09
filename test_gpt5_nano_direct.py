#!/usr/bin/env python3
"""Test GPT-5-Nano directly with SVG content"""

import os
import json
import requests

api_key = os.environ.get('OPENAI_API_KEY')
if not api_key:
    print("ERROR: OPENAI_API_KEY not set")
    exit(1)

# Simple SVG content
svg_content = """<svg viewBox="0 0 400 300" xmlns="http://www.w3.org/2000/svg">
  <rect x="50" y="50" width="300" height="200" fill="none" stroke="black" stroke-width="2"/>
  <line x1="150" y1="50" x2="150" y2="250" stroke="black" stroke-width="2"/>
  <line x1="250" y1="50" x2="250" y2="250" stroke="black" stroke-width="2"/>
  <line x1="50" y1="150" x2="350" y2="150" stroke="black" stroke-width="2"/>
</svg>"""

system_prompt = """You are an architectural SVG parser. Parse the SVG to extract wall and door segments.

For walls: long straight lines, is_load_bearing: true

For doors: short lines or gaps, is_load_bearing: false, interpolate if needed by connecting endpoints.

Output ONLY JSON: { "walls": [ {"start": {"x": f64, "y": f64}, "end": {"x": f64, "y": f64}, "is_load_bearing": bool } ] }

Use SVG coordinate system. Filter lines <5 units."""

payload = {
    "model": "gpt-5-nano",
    "messages": [
        {"role": "system", "content": system_prompt},
        {"role": "user", "content": f"SVG content:\n{svg_content}"}
    ],
    "response_format": {"type": "json_object"},
    "max_completion_tokens": 4096
}

print("Testing GPT-5-Nano SVG parsing...")
print("="*60)
print(f"SVG content:\n{svg_content}\n")
print("Sending request to OpenAI API...")

response = requests.post(
    "https://api.openai.com/v1/chat/completions",
    headers={
        "Authorization": f"Bearer {api_key}",
        "Content-Type": "application/json"
    },
    json=payload,
    timeout=60
)

print(f"\nStatus code: {response.status_code}")

if response.status_code == 200:
    data = response.json()
    print(f"\n✓ SUCCESS!\n")
    print(f"Usage:")
    print(json.dumps(data['usage'], indent=2))

    content = data['choices'][0]['message']['content']
    print(f"\nResponse content:")
    print(content)

    # Parse the JSON
    parsed = json.loads(content)
    print(f"\nParsed {len(parsed.get('walls', []))} walls:")
    for i, wall in enumerate(parsed.get('walls', [])):
        print(f"  {i+1}. ({wall['start']['x']}, {wall['start']['y']}) → ({wall['end']['x']}, {wall['end']['y']}) [load_bearing={wall['is_load_bearing']}]")
else:
    print(f"\n✗ ERROR!")
    print(response.text)
