# Floorplan Room Detector

A comprehensive Rust-based system for detecting rooms in architectural floorplan blueprints using graph-based cycle detection and AWS Textract integration.

## Project Structure

This is a Cargo workspace with multiple crates:

```
blueprint/
├── hf-floorplan-loader/    # HuggingFace dataset loader
├── axum-backend/            # Axum REST API server
├── leptos-frontend/         # Leptos WASM frontend
├── validation-pipeline/     # AWS Textract validation pipeline
└── tauri-stretch/           # Tauri native app (future)
```

## Features

### 🎯 Core Functionality

- **Graph-Based Room Detection**: Uses petgraph to build a graph from blueprint lines and detect enclosed rooms via cycle detection
- **Real Floorplan Processing**: Integrates 101 real apartment floorplans from HuggingFace dataset
- **AWS Textract Integration**: Extracts architectural lines from floorplan images using AWS Textract
- **Interactive Frontend**: Leptos-based WASM frontend with file upload and canvas rendering
- **REST API**: Axum backend with CORS support for room detection

### 📦 Components

#### 1. HuggingFace Dataset Loader (`hf-floorplan-loader`)

Loads and processes 101 real floorplan images with metadata:

```rust
use hf_floorplan_loader::FloorplanDataset;

// Load dataset from HuggingFace cache
let mut dataset = FloorplanDataset::new()?;

// Iterate through floorplans
for floorplan in dataset {
    println!("{}: {}", floorplan.file_name, floorplan.description);
}

// Batch processing
let batch = dataset.batch(10);

// Train/val/test split
let (train, val, test) = dataset.split(0.8, 0.1);
```

**Features:**
- Automatic dataset discovery in `~/.cache/huggingface/`
- Metadata CSV parsing with detailed room descriptions
- Image loading and validation
- Batch processing and dataset splitting
- Iterator implementation for easy traversal

#### 2. Axum Backend (`axum-backend`)

REST API server for room detection:

**Endpoints:**
- `GET /health` - Health check
- `POST /detect` - Detect rooms from blueprint lines

**Example Request:**
```json
POST /detect
{
  "lines": [
    {
      "start": {"x": 0.0, "y": 0.0},
      "end": {"x": 100.0, "y": 0.0}
    }
  ],
  "area_threshold": 100.0
}
```

**Example Response:**
```json
{
  "total_rooms": 2,
  "rooms": [
    {
      "id": 0,
      "bounding_box": [0.0, 0.0, 100.0, 100.0],
      "area": 10000.0,
      "name_hint": "Living Room",
      "points": [...]
    }
  ]
}
```

**Algorithm:**
1. Build graph from lines (nodes = points, edges = lines)
2. Find all cycles using DFS
3. Calculate polygon area for each cycle
4. Filter by area threshold
5. Compute bounding boxes
6. Generate heuristic room names

#### 3. Leptos Frontend (`leptos-frontend`)

Interactive WASM frontend with:
- File upload for JSON blueprints
- Real-time canvas rendering
- Detected room visualization
- Configurable area threshold
- Responsive design

**Running the frontend:**
```bash
cd leptos-frontend
trunk serve
```

Visit `http://localhost:8080`

#### 4. Validation Pipeline (`validation-pipeline`)

AWS Textract integration for processing real floorplan images:

```bash
cargo run --bin validation-pipeline
```

**Features:**
- Loads HuggingFace floorplan dataset
- Processes images with AWS Textract
- Extracts architectural lines from images
- Generates validation reports
- Compares against ground-truth metadata

**Output:**
```
🚀 Starting Floorplan Validation Pipeline
📁 Loading HuggingFace floorplan dataset...
✅ Loaded 101 floorplan images

🔧 Initializing AWS Textract client...
✅ AWS Textract client ready

Processing 1/5: 0.jpg
  ✅ Extracted 42 lines

📊 Validation Report
Total Processed: 5
Successful: 4 (80.0%)
Failed: 1 (20.0%)
```

## Getting Started

### Prerequisites

- Rust 1.80+
- Trunk (for frontend): `cargo install trunk`
- Just (task runner): `cargo install just`
- AWS credentials (for Textract)

### Installation

```bash
# Clone the repository
git clone <repo-url>
cd blueprint

# Check workspace builds
cargo check --workspace

# Run backend
cargo run --bin axum-backend

# Run frontend (in another terminal)
cd leptos-frontend
trunk serve

# Run validation pipeline
cargo run --bin validation-pipeline
```

### Using Just (Task Runner)

```bash
# View all available commands
just

# Build everything
just build

# Run backend
just run-backend

# Build frontend
just build-frontend

# Run tests
just test

# Run benchmarks
just bench
```

## Algorithm Details

### Room Detection Algorithm

1. **Graph Construction**:
   - Create nodes for each unique point
   - Create edges for each line connecting points
   - Handle floating-point precision with rounding

2. **Cycle Detection**:
   - Use DFS to find all simple cycles
   - Deduplicate cycles (same room, different starting points)
   - Filter cycles with < 3 points

3. **Area Calculation**:
   - Convert cycle points to polygon
   - Use Shoelace formula via `geo` crate
   - Filter by area threshold

4. **Room Classification**:
   - Calculate bounding box
   - Compute aspect ratio
   - Apply heuristics for room naming:
     - < 500 sq units: Small Room
     - 500-2000 sq units: Bedroom/Corridor (based on aspect ratio)
     - 2000-5000 sq units: Living Room
     - > 5000 sq units: Large Room

### Textract Parsing

1. **Line Extraction**:
   - Parse LINE blocks from Textract response
   - Extract horizontal/vertical lines based on aspect ratio
   - Convert normalized coordinates to pixel space

2. **Layout Figure Processing**:
   - Extract LAYOUT_FIGURE blocks (rooms, doors)
   - Extract edges as potential walls
   - Create line segments from bounding boxes

## Testing

```bash
# Run all tests
cargo test --workspace

# Run specific crate tests
cargo test -p hf-floorplan-loader
cargo test -p axum-backend

# Run with output
cargo test -- --nocapture
```

## Benchmarks

```bash
# Run all benchmarks
cargo bench --workspace

# Graph construction benchmark
cargo bench -p axum-backend --bench graph_construction
```

## Data Format

### Input JSON Format

```json
[
  {
    "start": {"x": 0.0, "y": 0.0},
    "end": {"x": 100.0, "y": 0.0},
    "is_load_bearing": false
  }
]
```

### HuggingFace Dataset

Location: `~/.cache/huggingface/hub/datasets--umesh16071973--New_Floorplan_demo_dataset/`

Contains:
- 101 floorplan images (0.jpg - 100.jpg)
- metadata.csv with room descriptions

Example metadata entry:
```csv
file_name,text
0.jpg,"a 3 room apartment with orientation in the north direction with a bedroom on the left a kitchen in the middle and a bathroom on the right with a hall attached to it"
```

## Architecture

```
┌─────────────────┐
│ Leptos Frontend │
│   (WASM/CSR)    │
└────────┬────────┘
         │ HTTP
         ▼
┌─────────────────┐      ┌──────────────┐
│  Axum Backend   │◄────►│  petgraph    │
│   (REST API)    │      │ nalgebra/geo │
└────────┬────────┘      └──────────────┘
         │
         ▼
┌─────────────────┐      ┌──────────────┐
│   Validation    │◄────►│ AWS Textract │
│    Pipeline     │      └──────────────┘
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  HuggingFace    │
│    Dataset      │
└─────────────────┘
```

## Dependencies

### Core Libraries
- **petgraph**: Graph data structures and algorithms
- **nalgebra**: Linear algebra for geometry calculations
- **geo**: Geometric algorithms (area, polygons)
- **aws-sdk-textract**: AWS Textract integration

### Web Stack
- **axum**: Web framework
- **leptos**: Reactive WASM frontend framework
- **tower**: Middleware
- **reqwest**: HTTP client

### Data Processing
- **serde/serde_json**: Serialization
- **csv**: CSV parsing
- **image**: Image loading

### Dev Tools
- **criterion**: Benchmarking
- **tracing**: Logging

## Performance

- Graph construction: O(N) where N = number of lines
- Cycle detection: O(V + E) where V = vertices, E = edges
- Area calculation: O(P) where P = points in polygon
- Memory efficient: Uses reference counting and iterators

## Future Enhancements

- [ ] Tauri native app for desktop/mobile
- [ ] Drag-and-drop room adjustment in frontend
- [ ] ML-based room classification
- [ ] Door and window detection
- [ ] 3D visualization
- [ ] Export to CAD formats
- [ ] Multi-floor support
- [ ] Room labeling from metadata

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Run tests: `cargo test --workspace`
5. Submit a pull request

## License

MIT License - see LICENSE file for details

## Acknowledgments

- HuggingFace dataset by umesh16071973
- AWS Textract for OCR/layout analysis
- Leptos community for WASM framework
- Rust ecosystem for amazing libraries
