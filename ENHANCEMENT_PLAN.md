# Floorplan Detection Enhancement Plan

## Overview
Enhance the existing graph-based room detection with:
1. Door/gap handling for incomplete wall cycles
2. Vision LLM integration (GPT-5) for intelligent room classification
3. Pre-trained model support (HuggingFace FloorPlanVisionAI)
4. Custom YOLOv8 training on CubiCasa5k dataset

## Phase 1: Door Detection & Gap Handling

### Problem
Current system requires complete wall cycles. Real floorplans have:
- Doorways (gaps in walls)
- Open floor plans
- Partial walls (kitchen counters, etc.)

### Solution: Gap-Tolerant Cycle Detection

```rust
// New structure to represent wall segments with gap tolerance
struct WallSegment {
    line: Line,
    has_door: bool,
    door_position: Option<f64>, // 0.0-1.0 along the line
    gap_width: Option<f64>,
}

// Enhanced graph builder
fn build_graph_with_gaps(
    lines: &[Line],
    max_gap_distance: f64,
) -> Graph<Point, WallSegment> {
    // 1. Identify potential door locations (line endpoint gaps)
    // 2. Create virtual connections across small gaps
    // 3. Build graph with gap edges marked
    // 4. Detect cycles allowing some gap edges
}
```

### Implementation Steps
1. Add `door_threshold` parameter to API (default: 50 units)
2. Detect gaps between nearby line endpoints
3. Create "virtual" edges for small gaps
4. Mark cycles as "has_door" if they use virtual edges
5. Update room detection to accept partial enclosures

**Files to modify:**
- `axum-backend/src/graph_builder.rs`
- `axum-backend/src/room_detector.rs`
- `axum-backend/src/main.rs` (add door_threshold param)

## Phase 2: GPT-5 Vision Integration

### Use Case
Analyze actual floorplan images to:
- Classify room types (bedroom, kitchen, bathroom, etc.)
- Detect furniture and fixtures
- Identify architectural features
- Generate detailed descriptions

### Implementation

```rust
// New crate: vision-classifier
use reqwest::Client;
use serde_json::json;
use base64::{Engine as _, engine::general_purpose};

struct VisionClassifier {
    client: Client,
    api_key: String,
}

impl VisionClassifier {
    async fn classify_room(
        &self,
        image_bytes: &[u8],
        detected_rooms: &[Room],
    ) -> Result<Vec<RoomClassification>, Error> {
        // 1. Encode image to base64
        let b64_image = general_purpose::STANDARD.encode(image_bytes);

        // 2. Create GPT-5 vision prompt
        let prompt = format!(
            "Analyze this floorplan and classify the {} detected rooms. \
             For each room, identify: type (bedroom/kitchen/bathroom/living/etc.), \
             likely furniture, and any special features.",
            detected_rooms.len()
        );

        // 3. Call OpenAI API
        let response = self.client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&json!({
                "model": "gpt-5",
                "messages": [{
                    "role": "user",
                    "content": [
                        {"type": "text", "text": prompt},
                        {
                            "type": "image_url",
                            "image_url": {
                                "url": format!("data:image/png;base64,{}", b64_image)
                            }
                        }
                    ]
                }],
                "max_tokens": 1000
            }))
            .send()
            .await?;

        // 4. Parse and return classifications
        parse_gpt5_response(response).await
    }
}
```

**New workspace member:**
- `vision-classifier/` - GPT-5 integration
- Dependencies: `reqwest`, `base64`, `serde_json`

### API Enhancement

```rust
// New endpoint
#[derive(Deserialize)]
struct VisionClassifyRequest {
    image_base64: String,
    lines: Vec<Line>,
    use_gpt5: bool,
}

async fn vision_classify_handler(
    Json(request): Json<VisionClassifyRequest>,
) -> Result<Json<VisionClassifyResponse>, Error> {
    // 1. Detect rooms using graph method
    let rooms = detect_rooms_from_lines(&request.lines);

    // 2. If use_gpt5, enhance with vision LLM
    let classified = if request.use_gpt5 {
        let classifier = VisionClassifier::new(env::var("OPENAI_API_KEY")?);
        classifier.classify_room(&decode_base64(&request.image_base64)?, &rooms).await?
    } else {
        rooms
    };

    Ok(Json(VisionClassifyResponse { rooms: classified }))
}
```

## Phase 3: HuggingFace Model Integration

### Model: FloorPlanVisionAIAdaptor
- **Source**: https://huggingface.co/sabaridsnfuji/FloorPlanVisionAIAdaptor
- **Type**: Vision adapter for room detection
- **Use**: Alternative to GPT-5, runs locally

### Implementation

```rust
// New crate: hf-vision-model
use candle_core::{Device, Tensor};
use candle_transformers::models::*;

struct FloorPlanVisionModel {
    model: VisionModel,
    tokenizer: Tokenizer,
    device: Device,
}

impl FloorPlanVisionModel {
    fn new() -> Result<Self> {
        // Load model from HuggingFace hub
        let api = hf_hub::api::sync::Api::new()?;
        let repo = api.model("sabaridsnfuji/FloorPlanVisionAIAdaptor".to_string());

        // Download weights
        let weights_path = repo.get("model.safetensors")?;

        // Initialize model
        let device = Device::cuda_if_available(0)?;
        let model = load_vision_model(&weights_path, &device)?;

        Ok(Self { model, tokenizer, device })
    }

    async fn detect_rooms(&self, image: &DynamicImage) -> Result<Vec<Room>> {
        // 1. Preprocess image
        let tensor = image_to_tensor(image, &self.device)?;

        // 2. Run inference
        let outputs = self.model.forward(&tensor)?;

        // 3. Post-process to extract rooms
        parse_model_outputs(outputs)
    }
}
```

**New dependencies:**
- `candle-core` - ML framework in Rust
- `candle-transformers` - Transformer models
- `hf-hub` - HuggingFace model downloading

**New workspace member:**
- `hf-vision-model/` - Local model inference

## Phase 4: YOLOv8 Training Pipeline

### Dataset: CubiCasa5k
- **Source**: https://www.kaggle.com/datasets/qmarva/cubicasa5k
- **Size**: 5,000 floorplan images with room annotations
- **Format**: Images + JSON annotations

### Training Setup

```python
# training/train_yolov8.py
from ultralytics import YOLO
import yaml

# Create dataset config
dataset_config = {
    'path': './data/cubicasa5k',
    'train': 'images/train',
    'val': 'images/val',
    'names': {
        0: 'wall',
        1: 'door',
        2: 'window',
        3: 'room',
        4: 'bathroom',
        5: 'bedroom',
        6: 'kitchen',
        7: 'living_room',
    }
}

with open('cubicasa_dataset.yaml', 'w') as f:
    yaml.dump(dataset_config, f)

# Load YOLOv8 small model
model = YOLO('yolov8s.pt')

# Train
results = model.train(
    data='cubicasa_dataset.yaml',
    epochs=100,  # Increase from Marcus's 20 for better results
    imgsz=640,
    batch=16,
    device=0,  # GPU
    patience=10,
    save=True,
    project='floorplan_detection',
    name='yolov8s_cubicasa'
)

# Export to ONNX for Rust inference
model.export(format='onnx')
```

### Rust Inference

```rust
// New crate: yolo-detector
use ort::{Session, SessionBuilder, Value};

struct YoloDetector {
    session: Session,
}

impl YoloDetector {
    fn new(model_path: &Path) -> Result<Self> {
        let session = SessionBuilder::new()?
            .with_model_from_file(model_path)?;
        Ok(Self { session })
    }

    fn detect_rooms(&self, image: &DynamicImage) -> Result<Vec<Detection>> {
        // 1. Preprocess image to 640x640
        let input = preprocess_image(image)?;

        // 2. Run YOLO inference
        let outputs = self.session.run(vec![Value::from_array(input)?])?;

        // 3. Post-process detections (NMS, etc.)
        let detections = postprocess_yolo_output(outputs)?;

        Ok(detections)
    }
}
```

**New dependencies:**
- `ort` (ONNX Runtime) - Fast inference
- `ndarray` - Array operations
- `image` - Image preprocessing

**New workspace member:**
- `yolo-detector/` - YOLO inference engine

## Phase 5: Unified Detection Pipeline

### Orchestration
Combine all methods for best results:

```rust
// New crate: unified-detector
pub struct UnifiedFloorplanDetector {
    graph_detector: GraphDetector,
    vision_classifier: Option<VisionClassifier>,
    hf_model: Option<FloorPlanVisionModel>,
    yolo_detector: Option<YoloDetector>,
}

pub struct DetectionConfig {
    use_graph: bool,
    use_gpt5: bool,
    use_hf_model: bool,
    use_yolo: bool,
    ensemble_strategy: EnsembleStrategy,
}

pub enum EnsembleStrategy {
    VoteByArea,      // Majority vote weighted by area overlap
    ConfidenceMax,   // Take highest confidence detection
    Cascade,         // Use methods in sequence (fast -> accurate)
}

impl UnifiedFloorplanDetector {
    pub async fn detect(
        &self,
        image: &DynamicImage,
        lines: Option<&[Line]>,
        config: DetectionConfig,
    ) -> Result<Vec<ClassifiedRoom>> {
        let mut all_detections = Vec::new();

        // 1. Graph-based (if lines provided)
        if config.use_graph && lines.is_some() {
            let graph_rooms = self.graph_detector.detect(lines.unwrap())?;
            all_detections.push(("graph", graph_rooms));
        }

        // 2. YOLO (fast, runs first)
        if config.use_yolo && self.yolo_detector.is_some() {
            let yolo_rooms = self.yolo_detector.as_ref().unwrap().detect_rooms(image)?;
            all_detections.push(("yolo", yolo_rooms));
        }

        // 3. HuggingFace model (medium speed)
        if config.use_hf_model && self.hf_model.is_some() {
            let hf_rooms = self.hf_model.as_ref().unwrap().detect_rooms(image).await?;
            all_detections.push(("hf", hf_rooms));
        }

        // 4. GPT-5 vision (slowest, most accurate classification)
        if config.use_gpt5 && self.vision_classifier.is_some() {
            let gpt5_rooms = self.vision_classifier.as_ref()
                .unwrap()
                .classify_room(&image_to_bytes(image)?, &all_detections)
                .await?;
            all_detections.push(("gpt5", gpt5_rooms));
        }

        // 5. Ensemble results
        self.ensemble_detections(all_detections, config.ensemble_strategy)
    }
}
```

## Implementation Priority

### Week 1: Door Detection (High Impact, Low Complexity)
- [ ] Implement gap-tolerant graph builder
- [ ] Add door_threshold parameter
- [ ] Update room detector for partial cycles
- [ ] Test with realistic floorplans

### Week 2: GPT-5 Integration (High Impact, Medium Complexity)
- [ ] Create vision-classifier crate
- [ ] Implement OpenAI API client
- [ ] Add vision endpoint to API
- [ ] Test with sample images from data/

### Week 3: YOLOv8 Training (High Impact, High Complexity)
- [ ] Download CubiCasa5k dataset
- [ ] Set up training environment
- [ ] Train YOLOv8s for 100 epochs
- [ ] Export to ONNX
- [ ] Create yolo-detector crate
- [ ] Implement inference pipeline

### Week 4: HF Model & Unification (Medium Impact, High Complexity)
- [ ] Set up candle-core infrastructure
- [ ] Download FloorPlanVisionAI model
- [ ] Implement inference
- [ ] Create unified-detector orchestration
- [ ] Benchmark all methods
- [ ] Optimize ensemble strategy

## Success Metrics

### Accuracy
- **Room Detection**: >90% IoU on test set
- **Room Classification**: >85% accuracy
- **Door Detection**: >95% recall

### Performance
- **Graph method**: <10ms per floorplan
- **YOLO inference**: <50ms per image
- **HF model**: <100ms per image
- **GPT-5 vision**: <2s per image (API latency)

### Ensemble
- **Combined accuracy**: >95% on CubiCasa5k test set
- **End-to-end latency**: <200ms (without GPT-5)

## References
- CubiCasa5k Paper: https://arxiv.org/abs/1904.01920
- YOLOv8 Docs: https://docs.ultralytics.com/
- Candle Framework: https://github.com/huggingface/candle
- ONNX Runtime Rust: https://github.com/pykeio/ort
