#!/usr/bin/env python3
"""
Python OpenCV-based room detection API wrapper.
Reads base64 image from stdin, outputs JSON to stdout.
"""
import cv2
import numpy as np
import json
import sys
import base64
from typing import List, Dict

def detect_rooms_from_base64(base64_image: str) -> List[Dict]:
    """Detect rooms from base64-encoded blueprint image using connected components."""
    try:
        # Decode base64 image
        img_data = base64.b64decode(base64_image)
        nparr = np.frombuffer(img_data, np.uint8)
        img = cv2.imdecode(nparr, cv2.IMREAD_GRAYSCALE)

        if img is None:
            return []

        # Threshold to binary: white rooms on black walls
        _, binary = cv2.threshold(img, 128, 255, cv2.THRESH_BINARY)

        # Find connected components
        num_labels, labels, stats, centroids = cv2.connectedComponentsWithStats(binary, connectivity=8)

        rooms = []
        min_area = 500
        max_area = img.shape[0] * img.shape[1] * 0.3

        # Find largest component for relative filtering
        areas = [stats[i, cv2.CC_STAT_AREA] for i in range(1, num_labels)]
        max_component_area = max(areas) if areas else 0
        size_threshold = max_component_area * 0.05  # 5% of largest

        room_id = 0
        for i in range(1, num_labels):  # Skip background
            area = stats[i, cv2.CC_STAT_AREA]

            # Apply both absolute and relative size filtering
            if area < min_area or area > max_area or area < size_threshold:
                continue

            x = stats[i, cv2.CC_STAT_LEFT]
            y = stats[i, cv2.CC_STAT_TOP]
            w = stats[i, cv2.CC_STAT_WIDTH]
            h = stats[i, cv2.CC_STAT_HEIGHT]

            # Calculate aspect ratio
            aspect_ratio = max(w, h) / max(min(w, h), 1)
            if aspect_ratio > 8.0:  # More lenient for blueprint rooms
                continue

            # Normalize coordinates (0-1000 scale)
            norm_x = (x / img.shape[1]) * 1000
            norm_y = (y / img.shape[0]) * 1000
            norm_w = (w / img.shape[1]) * 1000
            norm_h = (h / img.shape[0]) * 1000

            bbox = [norm_x, norm_y, norm_x + norm_w, norm_y + norm_h]

            # Create corner points
            points = [
                {"x": norm_x, "y": norm_y},
                {"x": norm_x + norm_w, "y": norm_y},
                {"x": norm_x + norm_w, "y": norm_y + norm_h},
                {"x": norm_x, "y": norm_y + norm_h}
            ]

            rooms.append({
                "id": room_id,
                "bounding_box": bbox,
                "area": float(area),
                "name_hint": "Large Room" if area > 20000 else ("Medium Room" if area > 5000 else "Small Room"),
                "points": points
            })
            room_id += 1

        return rooms
    except Exception as e:
        print(f"Error: {str(e)}", file=sys.stderr)
        return []

def main():
    try:
        # Read base64 image from stdin
        input_data = json.load(sys.stdin)
        base64_image = input_data.get("image", "")

        if not base64_image:
            print(json.dumps({"rooms": [], "lines": []}))
            return

        rooms = detect_rooms_from_base64(base64_image)

        # Output JSON to stdout
        output = {
            "rooms": rooms,
            "lines": []  # Python CC doesn't extract lines
        }
        print(json.dumps(output))
    except Exception as e:
        print(f"Fatal error: {str(e)}", file=sys.stderr)
        print(json.dumps({"rooms": [], "lines": []}))
        sys.exit(1)

if __name__ == "__main__":
    main()
