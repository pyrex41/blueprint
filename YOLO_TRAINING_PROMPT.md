# YOLO Training Agent Prompt - CubiCasa5k Floorplan Dataset

## 🎯 Mission

Train a YOLOv8 small model on the CubiCasa5k dataset for floorplan room detection, following Marcus's successful approach (20 epochs showed good results, but train for 100 for production quality).

---

## 📋 Task Checklist

### Phase 1: Dataset Setup
- [ ] Download CubiCasa5k from Kaggle: https://www.kaggle.com/datasets/qmarva/cubicasa5k
- [ ] Extract and organize dataset into YOLO format
- [ ] Parse annotations and convert to YOLO format (class, x_center, y_center, width, height)
- [ ] Create train/val/test split (80/10/10)
- [ ] Generate `dataset.yaml` configuration file
- [ ] Verify dataset integrity (check image-annotation pairs)

### Phase 2: Environment Setup
- [ ] Install dependencies: `pip install ultralytics opencv-python pillow pyyaml`
- [ ] Verify GPU availability: `torch.cuda.is_available()`
- [ ] Set up training directory structure
- [ ] Configure logging and checkpoints

### Phase 3: Model Training
- [ ] Load YOLOv8 small pretrained: `YOLO('yolov8s.pt')`
- [ ] Configure training hyperparameters
- [ ] Train for 100 epochs (can start with 20 for quick test)
- [ ] Monitor training metrics (mAP, loss curves)
- [ ] Save best model checkpoint

### Phase 4: Export & Integration
- [ ] Export trained model to ONNX format for Rust inference
- [ ] Validate ONNX model works
- [ ] Create sample inference script
- [ ] Document model performance metrics

---

## 📊 Dataset Information

### CubiCasa5k Dataset
- **Source**: https://www.kaggle.com/datasets/qmarva/cubicasa5k
- **Size**: 5,000 high-quality floorplan images with annotations
- **Format**: Images + annotations (need to convert to YOLO format)
- **Classes**: We need to detect and classify:
  - Rooms (bedroom, kitchen, bathroom, living_room, dining_room, etc.)
  - Walls
  - Doors
  - Windows

### Expected Directory Structure
```
data/
├── cubicasa5k/
│   ├── images/
│   │   ├── train/
│   │   ├── val/
│   │   └── test/
│   ├── labels/
│   │   ├── train/
│   │   ├── val/
│   │   └── test/
│   └── dataset.yaml
```

### dataset.yaml Template
```yaml
path: /path/to/cubicasa5k
train: images/train
val: images/val
test: images/test

# Classes (adjust based on CubiCasa5k annotations)
nc: 8  # number of classes
names: ['wall', 'door', 'window', 'room', 'bathroom', 'bedroom', 'kitchen', 'living_room']
```

---

## 🔧 Training Configuration

### Recommended Hyperparameters

```python
from ultralytics import YOLO

# Load pretrained YOLOv8 small model
model = YOLO('yolov8s.pt')

# Training configuration
training_args = {
    'data': 'cubicasa5k/dataset.yaml',
    'epochs': 100,           # Start with 20 for testing, 100 for production
    'imgsz': 640,            # Image size
    'batch': 16,             # Batch size (adjust based on GPU memory)
    'device': 0,             # GPU device (use 'cpu' if no GPU)
    'workers': 8,            # Data loading workers
    'project': 'floorplan_detection',
    'name': 'yolov8s_cubicasa_100ep',
    'exist_ok': True,
    'pretrained': True,
    'optimizer': 'AdamW',
    'lr0': 0.01,             # Initial learning rate
    'lrf': 0.01,             # Final learning rate factor
    'momentum': 0.937,
    'weight_decay': 0.0005,
    'warmup_epochs': 3,
    'warmup_momentum': 0.8,
    'warmup_bias_lr': 0.1,
    'box': 7.5,              # Box loss weight
    'cls': 0.5,              # Class loss weight
    'dfl': 1.5,              # DFL loss weight
    'patience': 10,          # Early stopping patience
    'save': True,
    'save_period': 10,       # Save checkpoint every N epochs
    'cache': True,           # Cache images for faster training
    'amp': True,             # Automatic Mixed Precision
    'mosaic': 1.0,           # Mosaic augmentation
    'mixup': 0.1,            # Mixup augmentation
    'copy_paste': 0.1,       # Copy-paste augmentation
    'degrees': 10.0,         # Rotation augmentation
    'translate': 0.1,        # Translation augmentation
    'scale': 0.5,            # Scale augmentation
    'shear': 2.0,            # Shear augmentation
    'flipud': 0.5,           # Vertical flip probability
    'fliplr': 0.5,           # Horizontal flip probability
    'hsv_h': 0.015,          # HSV hue augmentation
    'hsv_s': 0.7,            # HSV saturation augmentation
    'hsv_v': 0.4,            # HSV value augmentation
}

# Train the model
results = model.train(**training_args)
```

---

## 📝 Complete Training Script

Create `training/train_yolov8.py`:

```python
#!/usr/bin/env python3
"""
YOLOv8 Training Script for CubiCasa5k Floorplan Dataset

Usage:
    python train_yolov8.py --epochs 100 --batch 16 --device 0
"""

import argparse
import torch
from ultralytics import YOLO
from pathlib import Path

def main(args):
    print("🚀 Starting YOLOv8 Floorplan Detection Training")
    print("=" * 60)

    # Check GPU availability
    if torch.cuda.is_available():
        print(f"✅ GPU Available: {torch.cuda.get_device_name(0)}")
        print(f"   GPU Memory: {torch.cuda.get_device_properties(0).total_memory / 1e9:.2f} GB")
    else:
        print("⚠️  No GPU detected, training will be slow on CPU")
        if args.device != 'cpu':
            print("   Switching to CPU...")
            args.device = 'cpu'

    # Load pretrained model
    print(f"\n📦 Loading YOLOv8 {args.model} pretrained model...")
    model = YOLO(f'yolov8{args.model}.pt')

    # Verify dataset exists
    dataset_path = Path(args.data)
    if not dataset_path.exists():
        raise FileNotFoundError(f"Dataset not found at: {dataset_path}")

    print(f"📁 Dataset: {dataset_path}")
    print(f"🎯 Training for {args.epochs} epochs")
    print(f"📊 Batch size: {args.batch}")
    print(f"🖼️  Image size: {args.imgsz}")

    # Training configuration
    training_config = {
        'data': str(dataset_path),
        'epochs': args.epochs,
        'imgsz': args.imgsz,
        'batch': args.batch,
        'device': args.device,
        'workers': args.workers,
        'project': 'floorplan_detection',
        'name': f'yolov8{args.model}_cubicasa_{args.epochs}ep',
        'exist_ok': True,
        'pretrained': True,
        'optimizer': 'AdamW',
        'lr0': args.lr,
        'patience': args.patience,
        'save': True,
        'save_period': 10,
        'cache': args.cache,
        'amp': True,
        'verbose': True,
    }

    # Start training
    print("\n🏋️  Starting training...\n")
    results = model.train(**training_config)

    # Print results
    print("\n✅ Training Complete!")
    print("=" * 60)
    print(f"📊 Best mAP50: {results.results_dict.get('metrics/mAP50(B)', 'N/A')}")
    print(f"📊 Best mAP50-95: {results.results_dict.get('metrics/mAP50-95(B)', 'N/A')}")

    # Export to ONNX
    print("\n📤 Exporting model to ONNX format...")
    onnx_path = model.export(format='onnx', simplify=True)
    print(f"✅ ONNX model saved to: {onnx_path}")

    # Save metrics
    metrics_file = Path('floorplan_detection') / f'yolov8{args.model}_cubicasa_{args.epochs}ep' / 'metrics.txt'
    metrics_file.parent.mkdir(parents=True, exist_ok=True)

    with open(metrics_file, 'w') as f:
        f.write(f"YOLOv8{args.model} CubiCasa5k Training Results\n")
        f.write(f"{'=' * 60}\n")
        f.write(f"Epochs: {args.epochs}\n")
        f.write(f"Batch Size: {args.batch}\n")
        f.write(f"Image Size: {args.imgsz}\n")
        f.write(f"Best mAP50: {results.results_dict.get('metrics/mAP50(B)', 'N/A')}\n")
        f.write(f"Best mAP50-95: {results.results_dict.get('metrics/mAP50-95(B)', 'N/A')}\n")

    print(f"📝 Metrics saved to: {metrics_file}")

    return model, results

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description='Train YOLOv8 on CubiCasa5k dataset')

    # Model configuration
    parser.add_argument('--model', type=str, default='s', choices=['n', 's', 'm', 'l', 'x'],
                        help='YOLOv8 model size (n=nano, s=small, m=medium, l=large, x=xlarge)')
    parser.add_argument('--data', type=str, default='cubicasa5k/dataset.yaml',
                        help='Path to dataset.yaml')

    # Training hyperparameters
    parser.add_argument('--epochs', type=int, default=100,
                        help='Number of epochs (Marcus: 20 works, 100 for production)')
    parser.add_argument('--batch', type=int, default=16,
                        help='Batch size (adjust based on GPU memory)')
    parser.add_argument('--imgsz', type=int, default=640,
                        help='Input image size')
    parser.add_argument('--device', type=str, default='0',
                        help='Device to train on (0 for GPU, cpu for CPU)')
    parser.add_argument('--workers', type=int, default=8,
                        help='Number of data loading workers')
    parser.add_argument('--lr', type=float, default=0.01,
                        help='Initial learning rate')
    parser.add_argument('--patience', type=int, default=10,
                        help='Early stopping patience')
    parser.add_argument('--cache', action='store_true',
                        help='Cache images for faster training')

    args = parser.parse_args()

    # Train
    model, results = main(args)

    print("\n🎉 All done! Model ready for inference.")
    print(f"   Use the ONNX model for Rust integration")
```

---

## 🔄 Data Preprocessing Script

Create `training/prepare_dataset.py`:

```python
#!/usr/bin/env python3
"""
Convert CubiCasa5k dataset to YOLO format

The CubiCasa5k dataset needs to be converted from its original format
to YOLO format (class x_center y_center width height - normalized)
"""

import json
import shutil
from pathlib import Path
from PIL import Image
import yaml

def convert_cubicasa_to_yolo(cubicasa_root: Path, output_root: Path):
    """
    Convert CubiCasa5k annotations to YOLO format

    Args:
        cubicasa_root: Path to downloaded CubiCasa5k dataset
        output_root: Path where YOLO-formatted dataset will be saved
    """
    print("🔄 Converting CubiCasa5k to YOLO format...")

    # Create output directories
    for split in ['train', 'val', 'test']:
        (output_root / 'images' / split).mkdir(parents=True, exist_ok=True)
        (output_root / 'labels' / split).mkdir(parents=True, exist_ok=True)

    # Define class mapping (adjust based on CubiCasa5k structure)
    # You'll need to inspect the dataset to determine exact class names
    class_map = {
        'wall': 0,
        'door': 1,
        'window': 2,
        'room': 3,
        'bathroom': 4,
        'bedroom': 5,
        'kitchen': 6,
        'living_room': 7,
    }

    # Process dataset
    # NOTE: This is a template - you'll need to adapt based on CubiCasa5k's actual format
    total_converted = 0

    for split in ['train', 'val', 'test']:
        split_dir = cubicasa_root / split
        if not split_dir.exists():
            print(f"⚠️  Split directory not found: {split_dir}")
            continue

        # Iterate through images (adjust path based on actual structure)
        for img_path in split_dir.glob('**/*.png'):
            # Find corresponding annotation
            ann_path = img_path.with_suffix('.json')  # Adjust extension if needed

            if not ann_path.exists():
                print(f"⚠️  No annotation for {img_path.name}")
                continue

            # Load image to get dimensions
            img = Image.open(img_path)
            img_width, img_height = img.size

            # Load annotations
            with open(ann_path) as f:
                annotations = json.load(f)

            # Convert annotations to YOLO format
            yolo_labels = []
            for ann in annotations.get('objects', []):  # Adjust key based on format
                # Extract bounding box (adjust based on format)
                # CubiCasa5k format: you'll need to inspect the actual structure
                class_name = ann.get('class', 'room')
                bbox = ann.get('bbox', [])  # [x_min, y_min, x_max, y_max]

                if class_name not in class_map:
                    continue

                class_id = class_map[class_name]

                # Convert to YOLO format (normalized center x, y, width, height)
                x_min, y_min, x_max, y_max = bbox
                x_center = ((x_min + x_max) / 2) / img_width
                y_center = ((y_min + y_max) / 2) / img_height
                width = (x_max - x_min) / img_width
                height = (y_max - y_min) / img_height

                yolo_labels.append(f"{class_id} {x_center} {y_center} {width} {height}")

            # Save image and labels
            out_img_path = output_root / 'images' / split / img_path.name
            out_label_path = output_root / 'labels' / split / img_path.stem + '.txt'

            shutil.copy(img_path, out_img_path)

            with open(out_label_path, 'w') as f:
                f.write('\n'.join(yolo_labels))

            total_converted += 1

    print(f"✅ Converted {total_converted} images")

    # Create dataset.yaml
    dataset_config = {
        'path': str(output_root.absolute()),
        'train': 'images/train',
        'val': 'images/val',
        'test': 'images/test',
        'nc': len(class_map),
        'names': list(class_map.keys())
    }

    with open(output_root / 'dataset.yaml', 'w') as f:
        yaml.dump(dataset_config, f, default_flow_style=False)

    print(f"✅ Created dataset.yaml")
    print(f"📊 Classes: {list(class_map.keys())}")

if __name__ == '__main__':
    import argparse

    parser = argparse.ArgumentParser(description='Convert CubiCasa5k to YOLO format')
    parser.add_argument('--input', type=str, required=True,
                        help='Path to CubiCasa5k dataset')
    parser.add_argument('--output', type=str, default='cubicasa5k',
                        help='Output directory for YOLO format')

    args = parser.parse_args()

    cubicasa_root = Path(args.input)
    output_root = Path(args.output)

    convert_cubicasa_to_yolo(cubicasa_root, output_root)
```

---

## 📊 Expected Results

Based on Marcus's experience and typical YOLO performance:

### Training Metrics
- **20 epochs** (quick test): ~85% mAP50, good for validation
- **100 epochs** (production): ~90% mAP50, best quality

### Performance Targets
- **Inference Speed**: 20-50ms per image on GPU
- **mAP50**: >85% (room detection)
- **mAP50-95**: >70% (strict metric)
- **Model Size**: ~20-25 MB (YOLOv8s)

### Success Criteria
✅ Model detects rooms with >85% accuracy
✅ Inference runs in <100ms per image
✅ ONNX export works for Rust integration
✅ Model generalizes to test set

---

## 🔗 Integration Path

After training, integrate into the Rust project:

### 1. Export ONNX Model
```python
model.export(format='onnx', simplify=True)
```

### 2. Create Rust Inference Crate
Add to `blueprint/yolo-detector/`:
```rust
use ort::{Session, SessionBuilder};

pub struct YoloDetector {
    session: Session,
}

impl YoloDetector {
    pub fn new(model_path: &Path) -> Result<Self> {
        let session = SessionBuilder::new()?
            .with_model_from_file(model_path)?;
        Ok(Self { session })
    }

    pub fn detect(&self, image: &DynamicImage) -> Result<Vec<Detection>> {
        // Preprocess, infer, postprocess
        todo!()
    }
}
```

### 3. Add to Benchmark Suite
Update `unified-detector/src/bin/benchmark.rs` to include YOLO method.

---

## 🎯 Deliverables

Please provide:

1. **Training Script**: `train_yolov8.py` (working code)
2. **Dataset Conversion**: `prepare_dataset.py` (adapted to CubiCasa5k)
3. **Trained Model**: `best.pt` and `best.onnx`
4. **Training Report**:
   - Training curves (loss, mAP)
   - Final metrics (mAP50, mAP50-95)
   - Sample predictions (visualized)
   - Performance benchmark
5. **Integration Guide**: How to use the ONNX model in Rust

---

## ⚡ Quick Start Commands

```bash
# 1. Download dataset
kaggle datasets download -d qmarva/cubicasa5k
unzip cubicasa5k.zip -d cubicasa5k_raw

# 2. Convert to YOLO format
python training/prepare_dataset.py --input cubicasa5k_raw --output cubicasa5k

# 3. Quick test (20 epochs)
python training/train_yolov8.py --epochs 20 --batch 16

# 4. Production training (100 epochs)
python training/train_yolov8.py --epochs 100 --batch 16 --cache

# 5. Export to ONNX (already done in training script)
# Model will be at: floorplan_detection/yolov8s_cubicasa_100ep/weights/best.onnx
```

---

## 💡 Tips from Marcus

- **20 epochs is enough** for initial validation
- Use **data augmentation** (mosaic, mixup)
- **Cache images** if you have RAM (faster training)
- Monitor **mAP50** as primary metric
- **YOLOv8s** is sweet spot (speed vs accuracy)

---

## 📞 Support

If you encounter issues:
1. Check CubiCasa5k dataset structure first
2. Verify annotation format matches conversion script
3. Start with small subset (100 images) to test pipeline
4. Monitor GPU memory usage (reduce batch size if OOM)

---

## 🎉 Success Criteria

You've succeeded when:
✅ Dataset is in YOLO format
✅ Model trains without errors
✅ mAP50 > 85% on validation set
✅ ONNX model exports successfully
✅ Inference runs in <100ms per image

Good luck! 🚀
