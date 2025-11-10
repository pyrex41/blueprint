import cv2
import numpy as np
import json
from typing import List, Dict

def detect_rooms_from_image(image_path: str) -> List[Dict]:
    """Detect rooms from blueprint image using connected components."""
    # Load image
    img = cv2.imread(image_path, cv2.IMREAD_GRAYSCALE)
    if img is None:
        print(f"Failed to load image: {image_path}")
        return []
    
    print(f"Image loaded: {img.shape}")
    
    # Threshold to binary: white rooms on black walls (assume blueprint is black/white)
    _, binary = cv2.threshold(img, 200, 255, cv2.THRESH_BINARY)  # Adjust threshold if needed
    
    # Find connected components
    num_labels, labels, stats, centroids = cv2.connectedComponentsWithStats(binary, connectivity=8)
    
    rooms = []
    min_area = 1000  # Filter small noise
    max_area = img.shape[0] * img.shape[1] * 0.3  # Max 30% of image
    
    for i in range(1, num_labels):  # Skip background (label 0)
        area = stats[i, cv2.CC_STAT_AREA]
        if min_area < area < max_area:
            x = stats[i, cv2.CC_STAT_LEFT]
            y = stats[i, cv2.CC_STAT_TOP]
            w = stats[i, cv2.CC_STAT_WIDTH]
            h = stats[i, cv2.CC_STAT_HEIGHT]
            
            # Normalize coordinates (0-1000 as per PRD mock data)
            norm_x = (x / img.shape[1]) * 1000
            norm_y = (y / img.shape[0]) * 1000
            norm_w = (w / img.shape[1]) * 1000
            norm_h = (h / img.shape[0]) * 1000
            
            bbox = [norm_x, norm_y, norm_x + norm_w, norm_y + norm_h]
            
            rooms.append({
                "id": f"room_{i:03d}",
                "bounding_box": bbox,
                "name_hint": "Unknown"  # Could integrate LLM for naming
            })
    
    print(f"Detected {len(rooms)} rooms")
    return rooms

def main():
    image_path = "test-data/test_blueprint_004.png"
    rooms = detect_rooms_from_image(image_path)
    
    output = {"rooms": rooms}
    with open("detected_rooms.json", "w") as f:
        json.dump(output, f, indent=2)
    
    print("Output saved to detected_rooms.json")

if __name__ == "__main__":
    main()