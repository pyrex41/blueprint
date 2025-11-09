#!/usr/bin/env python3
"""Test which GPT-5 model names work with OpenAI API"""

import os
import requests

api_key = os.environ.get('OPENAI_API_KEY')
if not api_key:
    print("ERROR: OPENAI_API_KEY not set")
    exit(1)

# Test different model names
test_models = [
    "gpt-5-nano",
    "gpt-5",
    "gpt-5-mini",
    "o3-mini",
    "gpt-4o-mini",
]

print("Testing model names with OpenAI API...")
print("="*60)

for model in test_models:
    print(f"\nTesting: {model}")

    payload = {
        "model": model,
        "messages": [{"role": "user", "content": "Say 'ok'"}],
        "max_tokens": 5
    }

    try:
        response = requests.post(
            "https://api.openai.com/v1/chat/completions",
            headers={
                "Authorization": f"Bearer {api_key}",
                "Content-Type": "application/json"
            },
            json=payload,
            timeout=10
        )

        if response.status_code == 200:
            print(f"  ✓ Valid model!")
        else:
            error_msg = response.json().get('error', {}).get('message', response.text)
            print(f"  ✗ Error: {error_msg[:100]}")
    except Exception as e:
        print(f"  ✗ Exception: {str(e)[:100]}")
