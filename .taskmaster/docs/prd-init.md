# Product Requirements Document (PRD): Location Detection AI with Rust/Leptos Stack

## 1. Introduction and Goal

### 1.1 Project Goal

The primary goal remains to automate the detection of room boundaries from architectural blueprints, reducing manual effort from minutes to seconds. With this PRD, we adapt the solution to a Rust/Leptos full-stack architecture: Leptos for a reactive, performant frontend (web-first, with Tauri stretch for native desktop/mobile), and Rust for backend math-heavy processing (graph algorithms, vector parsing). This stack ensures mathematical purity (immutability, type safety), high performance (<30s latency), and rapid iteration (Cargo builds, hot-reload).

### 1.2 Context

Innergy users manually trace rooms in CAD tools, a bottleneck we're solving in-house after an inadequate outsourced attempt. The Rust/Leptos choice leverages 2025 ecosystem maturity: Leptos (signals for live UI updates) replaces React for functional reactivity; Rust (Axum for APIs, nalgebra for geometry) handles precise computations offline-capable. Stretch: Tauri integration for native apps, enabling on-site blueprint annotation.

### 1.3 Scope and Assumptions

MVP: Web app for mock JSON blueprints → room bounding boxes → canvas overlay.
Stretch: Tauri for desktop/mobile (offline processing, file pickers).
Assumptions: AWS for optional ML (SageMaker); no proprietary data—use generated mocks.
Out of Scope: Full ML training (rule-based MVP; augment later).

## 2. Problem & Business Context

### 2.1 Problem Statement

Users spend 5+ minutes per floor plan drawing arbitrary room shapes on blueprints. We need a Rust-powered service to ingest vector/line data (JSON mocks or extracted from images/PDFs), compute enclosed rooms via graph algorithms, and output precise bounding boxes/polygons for frontend rendering.

### 2.2 Current State & Opportunity

Existing tool extracts room names post-manual drawing. Automating boundaries saves clicks and positions Innergy as a differentiator. Rust/Leptos accelerates dev: Compile-time safety prevents coord errors; Leptos signals enable instant UI feedback (e.g., drag-to-verify rooms).

### 2.3 Success Metrics (Impact)

User Efficiency: Reduce 10-room plan from 5min to <30s (end-to-end, measured via browser perf tools).
Accuracy: 90%+ IoU (Intersection over Union) on mock datasets.
Adoption: 50%+ user uptake in beta; track via analytics (e.g., PostHog integration).
Tech Health: 100% test coverage; <5s build times; Tauri stretch deployable in <1 week.

## 3. Proposed Solution: Rust/Leptos Location Detection Pipeline

### 3.1 Core Functional Requirements

The system MUST:

Accept blueprint input: Mock JSON lines (0-1000 normalized coords) or images/PDFs (via AWS Textract for line extraction).
Process via Rust: Build wall graph (petgraph crate), detect cycles (enclosed rooms), compute bounding boxes (nalgebra).
Output JSON: Array of rooms {id: str, bounding_box: [f64;4], name_hint: str}.
Render in Leptos: Reactive canvas (e.g., via web-sys or plotters) with draggable overlays.
Stretch: Tauri commands for native file I/O, offline compute.

### 3.2 System Flow (High-Level)

Frontend (Leptos): User uploads file/JSON → signals trigger API call or client-side preview.
Backend (Rust/Axum): Parse lines → graph construction → cycle detection → JSON response. Orchestrate AWS (boto3-rs for Textract/SageMaker).
Processing Pipeline:
Preprocess: Merge collinear lines (shapely-rs equiv).
Detect: NetworkX-like cycles via petgraph; filter by area (> threshold).
Post-process: Merge overlaps; heuristic names (e.g., area-based: "Hall" if >20% total).
Frontend Rendering: Leptos view with <canvas> + signals for box coords; drag events update state.
Stretch (Tauri): #[tauri::command] wraps backend fn; native dialogs for uploads; mobile touch for drags.
Output: Downloadable JSON/SVG of annotated blueprint.

### 3.3 User Stories

As a user, I upload a mock JSON blueprint and see auto-detected rooms overlaid in <10s.
As a dev, I hot-reload math tweaks and verify cycles in-browser.
As an engineer, I use Tauri app on iPad to process offline PDFs.
As an admin, I monitor latency via CloudWatch integration.

## 4. Technical Requirements and Constraints

### 4.1 Technical Stack

Language: Rust 1.80+ (stable, no nightly).
Frontend: Leptos 0.7+ (reactive signals, server-side rendering optional for SEO).
UI: Trunk for WASM builds; wasm-bindgen for canvas.
Viz: plotters or rapier2d for overlays (or web-sys for raw CanvasRenderingContext2d).
Backend: Axum 0.7+ for HTTP API (async, tower middleware for auth/logging).
Math: nalgebra (vectors/bounds), petgraph (graphs/cycles), geo (polygons).
Parsing: serde for JSON; pdf-extract or textract-rs for inputs.
AWS: aws-sdk-rust (Textract, SageMaker endpoints).
Stretch (Tauri): Tauri 2.0+ core; mobile targets via cargo-mobile.
Dev Tools: Cargo, just (task runner), Biome (linting), Criterion (benchmarks).
Database: Optional SQLite (sqlx) for caching processed blueprints.
No-Go: Unsafe blocks in math core; external JS deps beyond WASM necessities.

### 4.2 Performance Benchmarks

Latency: <30s end-to-end (target: <5s for mock JSON via client-side WASM).
Throughput: Handle 1000-line blueprints in <10s (benchmark with Criterion).
Bundle Size: Leptos WASM <500KB; Tauri binary <10MB.
Mobile: <20s on mid-range Android/iOS (Tauri webview perf).

### 4.3 Non-Functional Requirements

Security: Rust's ownership prevents leaks; Axum middleware for CORS/JWT.
Accessibility: Leptos ARIA attrs; keyboard-navigable overlays.
Testing: 80%+ coverage (cargo-tarpaulin); e2e with dioxus (Leptos-compatible).
Monitoring: Tracing crate to CloudWatch; error rates <1%.
Off-Limits: "Magic" ML without fallbacks; non-AWS cloud.

### 4.4 Data Models

Input (Mock Blueprint): Vec<Line> where struct Line { start: Point, end: Point, is_load_bearing: bool }; type Point = [f64; 2].
Output (Rooms): Vec<Room> where #[derive(Serialize)] struct Room { id: String, bounding_box: [f64; 4], name_hint: Option<String> }.
Normalized Coords: 0.0-1000.0 range; serde aliases for JSON.

## 5. Mock Data Strategy

Generation: Procedural Rust script (rand crate) for 50+ mocks: Random rectangles/L-shapes with noise.
Input Example:
```json
[
  {"type": "line", "start": [100.0, 100.0], "end": [500.0, 100.0], "is_load_bearing": false},
  // ... (8+ lines for 2 rooms)
]
```

Output Example:
```json
[
  {"id": "room_001", "bounding_box": [100.0, 100.0, 500.0, 300.0], "name_hint": "Entry Hall"},
  // ...
]
```

Validation: IoU script in Criterion; generate ground-truth polys via geo.

## 6. Project Deliverables and Timeline

### 6.1 Deliverables

Code Repository: GitHub with Cargo workspace (leptos-frontend, axum-backend, tauri-stretch); README with setup (just dev).
Demo: 2-min video: Web upload → rooms overlay; Tauri build → mobile drag demo.
Technical Writeup (2-3 pages): Methodology (petgraph cycles), benchmarks, AWS configs (e.g., Textract query).
API Docs: OpenAPI spec via utoipa; Leptos component tree.
Stretch Artifacts: Tauri .exe/APK; offline mode toggle.

### 6.2 Timeline (4-Week Sprint, Solo/Small Team)

| Phase | Duration | Key Milestones | Dependencies |
|-------|----------|----------------|--------------|
| Setup & Mocks | Week 1 | Cargo init; mock gen script; basic Leptos UI (upload form). | None |
| Core Math MVP | Week 2 | petgraph parser → bounding boxes; Axum endpoint; Leptos integration. | nalgebra/petgraph crates |
| Polish & Tests | Week 3 | AWS stubs; reactivity (signals for drags); benchmarks/IoU. | aws-sdk-rust |
| Stretch & Demo | Week 4 | Tauri scaffold; video/writeup; CI (Actions for builds). | Tauri CLI |

### 6.3 Risks & Mitigations

Risk: WASM Math Perf: Mitigate: Client-side for mocks; server for heavy.
Risk: Tauri Mobile Bugs: Mitigate: Desktop-first; test on emulators.
Risk: Learning Curve: Mitigate: Leptos book (2025 ed.); Rustlings for basics.

This PRD positions Rust/Leptos as the efficient, future-proof stack—web baseline ships fast, Tauri stretch unlocks natives. Next: Kickoff with a petgraph prototype?