import numpy as np
from scipy import ndimage
import matplotlib.pyplot as plt
import json

def simplify_floorplan_to_rects(image_path, threshold=128, kernel_size=10, min_area=5000):
    """
    Simplifies a floorplan image by fuzzifying (morphological closing to ignore small details like doors),
    then extracts bounding rectangles for the rooms.

    :param image_path: Path to the input image file.
    :param threshold: Grayscale threshold for binarization (0-255).
    :param kernel_size: Size of the square kernel for morphological closing (larger to close bigger gaps).
    :param min_area: Minimum pixel area for a room to be considered.
    :return: List of rectangles as (x, y, width, height), and the output JSON.
    """
    # Load image
    img = plt.imread(image_path)
    
    # Convert to grayscale if necessary
    if len(img.shape) == 3:
        if img.shape[2] == 4:  # RGBA
            img = img[:, :, :3]
        gray = np.dot(img[..., :3], [0.2989, 0.5870, 0.1140])  # Standard grayscale conversion
    else:
        gray = img
    
    # Normalize to 0-255
    if gray.max() <= 1.0:
        gray = (gray * 255.0).astype(np.uint8)
    else:
        gray = gray.astype(np.uint8)
    
    # Binarize: walls are below threshold (dark), 1 for walls, 0 for rooms
    binary = (gray < threshold).astype(np.uint8)
    
    # Morphological closing to close small gaps (doors, etc.)
    structure = np.ones((kernel_size, kernel_size), dtype=np.uint8)
    closed = ndimage.binary_closing(binary, structure=structure).astype(np.uint8)
    
    # Invert to label rooms (now 1 for rooms, 0 for walls)
    rooms = 1 - closed
    
    # Label connected components (rooms)
    labels, num_labels = ndimage.label(rooms)
    
    # Compute areas
    areas = ndimage.sum(rooms, labels, range(num_labels + 1))
    
    # Filter small areas
    mask = areas >= min_area
    rectangles = []
    for label in range(1, num_labels + 1):
        if mask[label]:
            # Find bounding box
            rows, cols = np.where(labels == label)
            if len(rows) > 0 and len(cols) > 0:
                min_y, max_y = rows.min(), rows.max()
                min_x, max_x = cols.min(), cols.max()
                width = max_x - min_x + 1
                height = max_y - min_y + 1
                rectangles.append((min_x, min_y, width, height))
    
    # Generate JSON output per PRD
    height_img, width_img = gray.shape
    rooms_json = []
    for i, (x, y, w, h) in enumerate(rectangles):
        # Normalize to 0-1000 (PRD spec)
        norm_x = (x / width_img) * 1000
        norm_y = (y / height_img) * 1000
        norm_w = (w / width_img) * 1000
        norm_h = (h / height_img) * 1000
        bbox = [norm_x, norm_y, norm_x + norm_w, norm_y + norm_h]
        
        rooms_json.append({
            "id": f"room_{i+1:03d}",
            "bounding_box": [round(v, 2) for v in bbox],
            "name_hint": "Unknown"
        })
    
    output = {"rooms": rooms_json}
    
    return rectangles, json.dumps(output, indent=2)

def main():
    image_path = "test-data/test_blueprint_004.png"
    rects, json_output = simplify_floorplan_to_rects(image_path, threshold=128, kernel_size=10, min_area=5000)
    
    print(f"Detected {len(rects)} rooms")
    print("JSON Output:")
    print(json_output)
    
    with open("detected_rooms.json", "w") as f:
        f.write(json_output)
    
    print("Saved to detected_rooms.json")

if __name__ == "__main__":
    main()