use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_meta::*;
use leptos_router::components::*;
use leptos_router::*;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use js_sys::Date;

mod canvas;
use canvas::*;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Line {
    pub start: Point,
    pub end: Point,
    #[serde(default)]
    pub is_load_bearing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Room {
    pub id: usize,
    pub bounding_box: [f64; 4],
    pub area: f64,
    pub name_hint: String,
    pub points: Vec<Point>,
    #[serde(default)]
    pub room_type: Option<String>,
    #[serde(default)]
    pub confidence: Option<f64>,
    #[serde(default)]
    pub features: Vec<String>,
    #[serde(default)]
    pub detection_method: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
enum DetectionStrategy {
    // JSON-based strategies
    Simple,
    GraphOnly,
    GraphWithVision,
    YoloOnly,
    BestAvailable,
    Ensemble,

    // Image-based strategies
    HybridVision,
    VTracerOnly,
    Gpt5Only,

    // SVG-based strategies
    SvgOnly,
    SvgWithVision,
}

impl DetectionStrategy {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Simple => "Simple",
            Self::GraphOnly => "GraphOnly",
            Self::GraphWithVision => "GraphWithVision",
            Self::YoloOnly => "YoloOnly",
            Self::BestAvailable => "BestAvailable",
            Self::Ensemble => "Ensemble",
            Self::HybridVision => "HybridVision",
            Self::VTracerOnly => "vtracer_only",
            Self::Gpt5Only => "gpt5_only",
            Self::SvgOnly => "SvgOnly",
            Self::SvgWithVision => "SvgWithVision",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            Self::Simple => "Vertical divider detection - Fast for rectangular layouts with left-right divisions",
            Self::GraphOnly => "Cycle detection - Handles complex polygons (pentagons, hexagons, L-shapes)",
            Self::GraphWithVision => "Geometric + AI classification - Best accuracy, slower (~54s)",
            Self::YoloOnly => "ML-based detection (not available yet)",
            Self::BestAvailable => "Auto-fallback to best available method",
            Self::Ensemble => "Run all methods, return best result",
            Self::HybridVision => "Hybrid Vision Pipeline - Combines VTracer + AI vision for blueprint vectorization",
            Self::VTracerOnly => "VTracer Only - Pure vectorization without AI classification",
            Self::Gpt5Only => "Vision Only - Pure AI-based blueprint analysis",
            Self::SvgOnly => "SVG Direct - Parse SVG files algorithmically, no AI required",
            Self::SvgWithVision => "SVG + Vision - Parse SVG + use AI for room classification",
        }
    }

    fn input_type(&self) -> InputType {
        match self {
            Self::Simple | Self::GraphOnly | Self::GraphWithVision | Self::YoloOnly | Self::BestAvailable | Self::Ensemble => InputType::Json,
            Self::HybridVision | Self::VTracerOnly | Self::Gpt5Only => InputType::Image,
            Self::SvgOnly | Self::SvgWithVision => InputType::Svg,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum InputType {
    Json,
    Image,
    Svg,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum SvgParser {
    Algorithmic,  // Fast geometric parsing
    Gpt5Nano,     // AI text model interprets SVG
    Combined,     // Run both parsers and compare
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ImageVisionChoice {
    NoVision,      // y paths: Vectorize only, then parse SVG
    VisionOnly,    // x path: Pure AI, skip SVG vectorization
    VisionWithSvg, // z paths: Hybrid - vectorize, parse, merge with AI vision
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum VisionMode {
    None,           // No vision enhancement
    Classification, // Add room type classification
    Hybrid,         // Merge vision walls with vectorized walls (images only)
    VisionOnly,     // Pure vision analysis (images only)
}

#[derive(Debug, Serialize)]
struct DetectRequest {
    lines: Vec<Line>,
    area_threshold: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    door_threshold: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    coverage_threshold: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    outer_boundary_ratio: Option<f64>,
    strategy: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    enable_vision: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    enable_yolo: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct DetectResponse {
    rooms: Vec<Room>,
    #[serde(default)]
    method_used: Option<String>,
    #[serde(default)]
    execution_time_ms: Option<u64>,
    #[serde(default)]
    metadata: Option<serde_json::Value>,
}

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    view! {
        <Stylesheet id="leptos" href="/pkg/leptos-frontend.css"/>
        <Title text="AI Floorplan Room Detector"/>
        <Router>
            <Routes fallback=|| "Not found">
                <Route path=path!("") view=DualAlgorithmDetector/>
                <Route path=path!("/old") view=FloorplanDetector/>
                <Route path=path!("/test") view=AlgorithmTest/>
            </Routes>
        </Router>
    }
}

#[component]
fn FloorplanDetector() -> impl IntoView {
    let lines = RwSignal::new(Vec::<Line>::new());
    let rooms = RwSignal::new(Vec::<Room>::new());
    let loading = RwSignal::new(false);
    let error = RwSignal::new(Option::<String>::None);
    let area_threshold = RwSignal::new(100.0);
    let door_threshold = RwSignal::new(50.0);
    let coverage_threshold = RwSignal::new(0.3); // 30% minimum coverage for dividers
    let outer_boundary_ratio = RwSignal::new(1.5); // 1.5x area ratio for boundary filtering
    let _strategy = RwSignal::new(DetectionStrategy::HybridVision); // Legacy - kept for compatibility
    let input_type = RwSignal::new(Option::<InputType>::None); // Auto-detect from file
    let svg_parser = RwSignal::new(SvgParser::Algorithmic); // Default parser
    let image_vision = RwSignal::new(ImageVisionChoice::VisionWithSvg); // Default hybrid for images
    let _vision_mode = RwSignal::new(VisionMode::None); // Legacy - kept for compatibility
    let uploaded_filename = RwSignal::new(Option::<String>::None); // Track uploaded file
    let method_used = RwSignal::new(Option::<String>::None);
    let execution_time = RwSignal::new(Option::<u64>::None);
    let svg_content = RwSignal::new(Option::<String>::None); // Store SVG content

    let file_input_ref = NodeRef::<leptos::html::Input>::new();
    let canvas_ref = NodeRef::<leptos::html::Canvas>::new();

    // Handle file upload (JSON, images, and SVG)
    let on_file_change = move |_| {
        if let Some(input) = file_input_ref.get() {
            if let Some(files) = input.files() {
                if let Some(file) = files.get(0) {
                    let file_type = file.type_();
                    let file_name = file.name();

                    // Auto-detect input type from file
                    let detected_type = if file_name.ends_with(".json") || file_type == "application/json" {
                        Some(InputType::Json)
                    } else if file_name.ends_with(".svg") || file_type == "image/svg+xml" {
                        Some(InputType::Svg)
                    } else if file_type.starts_with("image/") {
                        Some(InputType::Image)
                    } else {
                        error.set(Some("Unsupported file type. Please upload JSON, SVG, PNG, or JPG.".to_string()));
                        return;
                    };

                    input_type.set(detected_type);
                    uploaded_filename.set(Some(file_name.clone()));

                    match detected_type.unwrap() {
                        InputType::Json => {
                            // Handle JSON file
                            if file_name.ends_with(".json") || file_type == "application/json" {
                                let reader = web_sys::FileReader::new().unwrap();
                                let reader_clone = reader.clone();

                                let onload = Closure::wrap(Box::new(move |_event: web_sys::Event| {
                                    if let Ok(result) = reader_clone.result() {
                                        if let Some(text) = result.as_string() {
                                            match serde_json::from_str::<Vec<Line>>(&text) {
                                                Ok(parsed_lines) => {
                                                    lines.set(parsed_lines);
                                                    error.set(None);
                                                }
                                                Err(e) => {
                                                    error.set(Some(format!("Failed to parse JSON: {}", e)));
                                                }
                                            }
                                        }
                                    }
                                }) as Box<dyn FnMut(_)>);

                                reader.set_onload(Some(onload.as_ref().unchecked_ref()));
                                onload.forget();
                                let _ = reader.read_as_text(&file);
                            } else {
                                error.set(Some("Please upload a JSON file for JSON input type".to_string()));
                            }
                        }
                        InputType::Image => {
                            // Handle image file
                            if file_type.starts_with("image/") {
    // Handle image file - read as base64 and send to /vectorize-blueprint endpoint
    loading.set(true);
    error.set(None);

    let reader = web_sys::FileReader::new().unwrap();
    let reader_clone = reader.clone();

    // Get image processing choices
    let current_image_vision = image_vision.get();
    let current_svg_parser = svg_parser.get();

    let onload = Closure::wrap(Box::new(move |_event: web_sys::Event| {
        if let Ok(result) = reader_clone.result() {
            if let Some(data_url) = result.as_string() {
                // Extract base64 data (remove "data:image/...;base64," prefix)
                if let Some(base64_data) = data_url.split(',').nth(1) {
                    let base64_owned = base64_data.to_string();

                    spawn_local(async move {
                        #[derive(Serialize)]
                        struct VectorizeRequest {
                            image: String,
                            strategy: String,
                        }

                        // Image: Map vision choice + parser choice to backend strategy (7 paths)
                        let backend_strategy = match (current_image_vision, current_svg_parser) {
                            // x) Vision only - no SVG parsing
                            (ImageVisionChoice::VisionOnly, _) => "gpt5_only",

                            // y1-y3) No vision + SVG parser choices
                            (ImageVisionChoice::NoVision, SvgParser::Algorithmic) => "vtracer_only",
                            (ImageVisionChoice::NoVision, SvgParser::Gpt5Nano) => "vtracer_ai_parser",
                            (ImageVisionChoice::NoVision, SvgParser::Combined) => "vtracer_combined",

                            // z1-z3) Vision + SVG parser choices
                            (ImageVisionChoice::VisionWithSvg, SvgParser::Algorithmic) => "hybrid_vision",
                            (ImageVisionChoice::VisionWithSvg, SvgParser::Gpt5Nano) => "hybrid_ai_parser",
                            (ImageVisionChoice::VisionWithSvg, SvgParser::Combined) => "hybrid_combined",
                        };

                        let request = VectorizeRequest {
                            image: base64_owned,
                            strategy: backend_strategy.to_string(),
                        };

                        match vectorize_blueprint(request).await {
                            Ok(response) => {
                                // Extract rooms from response
                                if let Some(rooms_array) = response.get("rooms") {
                                    if let Ok(parsed_rooms) = serde_json::from_value::<Vec<Room>>(rooms_array.clone()) {
                                        rooms.set(parsed_rooms);
                                    }
                                }
                                // Extract walls for visualization (backend returns "walls" not "lines")
                                if let Some(walls_array) = response.get("walls") {
                                    // Convert walls to lines for display
                                    if let Ok(parsed_walls) = serde_json::from_value::<Vec<serde_json::Value>>(walls_array.clone()) {
                                        let wall_lines: Vec<Line> = parsed_walls
                                            .iter()
                                            .filter_map(|wall| {
                                                let start = wall.get("start")?;
                                                let end = wall.get("end")?;
                                                Some(Line {
                                                    start: Point {
                                                        x: start.get("x")?.as_f64()?,
                                                        y: start.get("y")?.as_f64()?,
                                                    },
                                                    end: Point {
                                                        x: end.get("x")?.as_f64()?,
                                                        y: end.get("y")?.as_f64()?,
                                                    },
                                                    is_load_bearing: false,
                                                })
                                            })
                                            .collect();
                                        lines.set(wall_lines);
                                    }
                                }
                                loading.set(false);
                                error.set(None);
                            }
                            Err(e) => {
                                error.set(Some(format!("Vectorization failed: {}", e)));
                                loading.set(false);
                            }
                        }
                    });
                }
            }
        }
    }) as Box<dyn FnMut(_)>);

    reader.set_onload(Some(onload.as_ref().unchecked_ref()));
    onload.forget();
    let _ = reader.read_as_data_url(&file);
} else {
                                error.set(Some("Please upload an image file for Image input type".to_string()));
                            }
                        }
                         InputType::Svg => {
                             // Handle SVG file
                             if file_name.ends_with(".svg") || file_type == "image/svg+xml" {
                                 let reader = web_sys::FileReader::new().unwrap();
                                 let reader_clone = reader.clone();

                                 let onload = Closure::wrap(Box::new(move |_event: web_sys::Event| {
                                     if let Ok(result) = reader_clone.result() {
                                         if let Some(svg_text) = result.as_string() {
                                             // Store the SVG content for later processing
                                             svg_content.set(Some(svg_text));
                                             lines.set(Vec::new());
                                             rooms.set(Vec::new());
                                             error.set(None);
                                         }
                                     }
                                 }) as Box<dyn FnMut(_)>);

                                 reader.set_onload(Some(onload.as_ref().unchecked_ref()));
                                 onload.forget();
                                 let _ = reader.read_as_text(&file);
                             } else {
                                 error.set(Some("Please upload an SVG file for SVG input type".to_string()));
                             }
                         }
                    }
                }
            }
        }
    };

    // Handle detection
    let on_detect = move |_| {
        let current_input_type = match input_type.get() {
            Some(t) => t,
            None => {
                error.set(Some("Please upload a file first".to_string()));
                return;
            }
        };

        // Validate input based on type
        match current_input_type {
            InputType::Json => {
                let current_lines = lines.get();
                if current_lines.is_empty() {
                    error.set(Some("Please upload a JSON file first".to_string()));
                    return;
                }
            }
            InputType::Image => {
                // Image handling is done in file upload, rooms are set there
                if rooms.get().is_empty() {
                    error.set(Some("Please upload an image file first".to_string()));
                    return;
                }
                // For images, detection already happened during upload
                return;
            }
            InputType::Svg => {
                let current_svg = svg_content.get();
                if current_svg.is_none() {
                    error.set(Some("Please upload an SVG file first".to_string()));
                    return;
                }
            }
        }

        loading.set(true);
        error.set(None);
        method_used.set(None);
        execution_time.set(None);

        let current_area_threshold = area_threshold.get();
        let current_door_threshold = door_threshold.get();

        spawn_local(async move {
            match current_input_type {
                InputType::Json => {
                    let current_lines = lines.get();
                    let current_coverage_threshold = coverage_threshold.get();
                    let current_outer_boundary_ratio = outer_boundary_ratio.get();

                    // JSON: Always use GraphOnly (no vision)
                    let backend_strategy = "GraphOnly".to_string();

                    let request = DetectRequest {
                        lines: current_lines,
                        area_threshold: current_area_threshold,
                        door_threshold: Some(current_door_threshold),
                        coverage_threshold: Some(current_coverage_threshold),
                        outer_boundary_ratio: Some(current_outer_boundary_ratio),
                        strategy: backend_strategy.clone(),
                        enable_vision: Some(false),
                        enable_yolo: Some(false),
                    };

                    match detect_rooms(request, DetectionStrategy::GraphOnly).await {
                        Ok(response) => {
                            rooms.set(response.rooms);
                            method_used.set(response.method_used);
                            execution_time.set(response.execution_time_ms);
                            loading.set(false);
                        }
                        Err(e) => {
                            error.set(Some(format!("Detection failed: {}", e)));
                            loading.set(false);
                        }
                    }
                }
                InputType::Svg => {
                    let current_svg = svg_content.get().unwrap();
                    let current_svg_parser = svg_parser.get();

                    // SVG: Map parser choice to backend strategy
                    // Note: Currently only SvgOnly (algorithmic) is fully implemented in backend
                    // AI Parser and Combined options will be added in future updates
                    let backend_strategy = DetectionStrategy::SvgOnly; // All options use algorithmic parsing for now
                    let enable_vision = false; // SVG vision support requires image rendering (not yet implemented)

                    match detect_svg_rooms(current_svg, current_area_threshold, current_door_threshold, backend_strategy, enable_vision).await {
                        Ok(response) => {
                            rooms.set(response.rooms);
                            method_used.set(response.method_used);
                            execution_time.set(response.execution_time_ms);
                            loading.set(false);
                        }
                        Err(e) => {
                            error.set(Some(format!("SVG detection failed: {}", e)));
                            loading.set(false);
                        }
                    }
                }
                InputType::Image => {
                    // Already handled above
                    loading.set(false);
                }
            }
        });
    };

    // Render canvas whenever lines or rooms change
    Effect::new(move |_| {
        let current_lines = lines.get();
        let current_rooms = rooms.get();

        if let Some(canvas) = canvas_ref.get_untracked() {
            render_floorplan(&canvas, &current_lines, &current_rooms);
        }
    });

    view! {
        <div class="container">
<header>
    <h1>"AI Floorplan Room Detector"</h1>
    <p>"Upload JSON lines, blueprint images (PNG/JPG), or vector SVGs for AI-powered room detection"</p>
</header>

            <div class="controls">
                // Step 1: Upload File (auto-detects type)
                <div class="file-upload-primary">
                    <h3>"Step 1: Upload Your Floorplan"</h3>
                    <input
                        type="file"
                        accept=".json,.svg,.png,.jpg,.jpeg"
                        node_ref=file_input_ref
                        on:change=on_file_change
                    />
                    <p class="file-info">
                        "Supported formats: JSON (lines), SVG (vector), PNG/JPG (images)"
                    </p>

                    {move || uploaded_filename.get().map(|filename| {
                        view! {
                            <div class="upload-success">
                                <strong>"✓ Uploaded: "</strong> {filename}
                                <br/>
                                <small>{
                                    match input_type.get() {
                                        Some(InputType::Json) => "Type: JSON Lines",
                                        Some(InputType::Image) => "Type: Blueprint Image",
                                        Some(InputType::Svg) => "Type: Vector SVG",
                                        None => "",
                                    }
                                }</small>
                            </div>
                        }
                    })}
                </div>

                // Step 2: Conditional Strategy Selection Based on File Type
                <div class="strategy-selector">
                    {move || {
                        match input_type.get() {
                            None => view! {
                                <p class="placeholder">"Upload a file to see detection options"</p>
                            }.into_any(),

                            Some(InputType::Json) => view! {
                                <div class="info-message">
                                    <p>"✓ JSON file detected - using GraphOnly strategy"</p>
                                    <small>"Graph cycle detection with geometric analysis"</small>
                                </div>
                            }.into_any(),
                            Some(InputType::Image) => view! {
                                <div class="image-strategy">
                                    <h3>"Step 2a: Vision Enhancement"</h3>
                                    <label class="strategy-option">
                                        <input
                                            type="radio"
                                            name="vision-choice"
                                            checked=move || matches!(image_vision.get(), ImageVisionChoice::NoVision)
                                            on:change=move |_| image_vision.set(ImageVisionChoice::NoVision)
                                        />
                                        <div class="option-content">
                                            <strong>"No Vision (y)"</strong>
                                            <p>"Vectorize only, then parse SVG"</p>
                                            <small>"⚡ Fast • 📐 Geometric only"</small>
                                        </div>
                                    </label>
                                    <label class="strategy-option">
                                        <input
                                            type="radio"
                                            name="vision-choice"
                                            checked=move || matches!(image_vision.get(), ImageVisionChoice::VisionOnly)
                                            on:change=move |_| image_vision.set(ImageVisionChoice::VisionOnly)
                                        />
                                        <div class="option-content">
                                            <strong>"Vision Only (x)"</strong>
                                            <p>"Pure AI, skip SVG vectorization"</p>
                                            <small>"🧠 AI-powered • ⏱️ Slower (~1-2min)"</small>
                                        </div>
                                    </label>
                                    <label class="strategy-option">
                                        <input
                                            type="radio"
                                            name="vision-choice"
                                            checked=move || matches!(image_vision.get(), ImageVisionChoice::VisionWithSvg)
                                            on:change=move |_| image_vision.set(ImageVisionChoice::VisionWithSvg)
                                        />
                                        <div class="option-content">
                                            <strong>"Vision + SVG Parsing (z)"</strong>
                                            <p>"Hybrid: vectorize, parse, and merge with AI vision"</p>
                                            <small>"🎯 Best accuracy • ⏱️ Moderate (~15-20s)"</small>
                                        </div>
                                    </label>

                                    // Step 2b: SVG parser choice (hidden if VisionOnly)
                                    {move || {
                                        if !matches!(image_vision.get(), ImageVisionChoice::VisionOnly) {
                                            view! {
                                                <div class="svg-parser-choice">
                                                    <h3>"Step 2b: Choose SVG Parser"</h3>
                                                    <label class="strategy-option">
                                                        <input
                                                            type="radio"
                                                            name="svg-parser"
                                                            checked=move || matches!(svg_parser.get(), SvgParser::Algorithmic)
                                                            on:change=move |_| svg_parser.set(SvgParser::Algorithmic)
                                                        />
                                                        <div class="option-content">
                                                            <strong>"a) Algorithmic"</strong>
                                                            <small>"Fast geometric parsing"</small>
                                                        </div>
                                                    </label>
                                                    <label class="strategy-option">
                                                        <input
                                                            type="radio"
                                                            name="svg-parser"
                                                            checked=move || matches!(svg_parser.get(), SvgParser::Gpt5Nano)
                                                            on:change=move |_| svg_parser.set(SvgParser::Gpt5Nano)
                                                        />
                                                        <div class="option-content">
                                                            <strong>"b) AI Parser"</strong>
                                                            <small>"AI text interpretation"</small>
                                                        </div>
                                                    </label>
                                                    <label class="strategy-option">
                                                        <input
                                                            type="radio"
                                                            name="svg-parser"
                                                            checked=move || matches!(svg_parser.get(), SvgParser::Combined)
                                                            on:change=move |_| svg_parser.set(SvgParser::Combined)
                                                        />
                                                        <div class="option-content">
                                                            <strong>"c) Combined"</strong>
                                                            <small>"Run both and compare"</small>
                                                        </div>
                                                    </label>
                                                </div>
                                            }.into_any()
                                        } else {
                                            view! {}.into_any()
                                        }
                                    }}
                                </div>
                            }.into_any(),
                            Some(InputType::Svg) => view! {
                                <div class="svg-parser-choice">
                                    <h3>"Step 2: Choose SVG Parser"</h3>
                                    <label class="strategy-option">
                                        <input
                                            type="radio"
                                            name="svg-parser"
                                            checked=move || matches!(svg_parser.get(), SvgParser::Algorithmic)
                                            on:change=move |_| svg_parser.set(SvgParser::Algorithmic)
                                        />
                                        <div class="option-content">
                                            <strong>"a) Algorithmic Parser"</strong>
                                            <p>"Fast geometric parsing of SVG elements"</p>
                                            <small>"⚡ Fastest (~1ms) • 📐 Geometric accuracy"</small>
                                        </div>
                                    </label>
                                    <label class="strategy-option">
                                        <input
                                            type="radio"
                                            name="svg-parser"
                                            checked=move || matches!(svg_parser.get(), SvgParser::Gpt5Nano)
                                            on:change=move |_| svg_parser.set(SvgParser::Gpt5Nano)
                                        />
                                        <div class="option-content">
                                            <strong>"b) AI Parser (GPT-5 Nano)"</strong>
                                            <p>"AI text model interprets SVG markup"</p>
                                            <small>"🧠 AI understanding • ⏱️ Variable time • 🚧 Coming Soon"</small>
                                        </div>
                                    </label>
                                    <label class="strategy-option">
                                        <input
                                            type="radio"
                                            name="svg-parser"
                                            checked=move || matches!(svg_parser.get(), SvgParser::Combined)
                                            on:change=move |_| svg_parser.set(SvgParser::Combined)
                                        />
                                        <div class="option-content">
                                            <strong>"c) Combined Parser"</strong>
                                            <p>"Run both parsers and compare results"</p>
                                            <small>"🔬 Validation • ⏱️ 2x time • 🚧 Coming Soon"</small>
                                        </div>
                                    </label>
                                </div>
                            }.into_any(),
                        }
                    }}
                </div>

                <div class="threshold-control">
                    <label for="threshold">"Area Threshold:"</label>
                    <input
                        type="number"
                        id="threshold"
                        prop:value=move || area_threshold.get()
                        on:input=move |ev| {
                            let val = ev.target().unwrap().unchecked_into::<web_sys::HtmlInputElement>().value();
                            if let Ok(parsed) = val.parse::<f64>() {
                                area_threshold.set(parsed);
                            }
                        }
                    />
                </div>

                <div class="threshold-control">
                    <label for="door-threshold">"Door Threshold:"</label>
                    <input
                        type="number"
                        id="door-threshold"
                        prop:value=move || door_threshold.get()
                        on:input=move |ev| {
                            let val = ev.target().unwrap().unchecked_into::<web_sys::HtmlInputElement>().value();
                            if let Ok(parsed) = val.parse::<f64>() {
                                door_threshold.set(parsed);
                            }
                        }
                    />
                    <p style="font-size: 11px; color: #666;">"For cycle detection (gap bridging)"</p>
                </div>

                <div class="threshold-control">
                    <label for="coverage-threshold">"Coverage Threshold:"</label>
                    <input
                        type="number"
                        id="coverage-threshold"
                        step="0.1"
                        min="0.0"
                        max="1.0"
                        prop:value=move || coverage_threshold.get()
                        on:input=move |ev| {
                            let val = ev.target().unwrap().unchecked_into::<web_sys::HtmlInputElement>().value();
                            if let Ok(parsed) = val.parse::<f64>() {
                                coverage_threshold.set(parsed);
                            }
                        }
                    />
                    <p style="font-size: 11px; color: #666;">"For simple algorithm (0.3 = 30% height)"</p>
                </div>

                <div class="threshold-control">
                    <label for="boundary-ratio">"Boundary Ratio:"</label>
                    <input
                        type="number"
                        id="boundary-ratio"
                        step="0.1"
                        min="1.0"
                        max="3.0"
                        prop:value=move || outer_boundary_ratio.get()
                        on:input=move |ev| {
                            let val = ev.target().unwrap().unchecked_into::<web_sys::HtmlInputElement>().value();
                            if let Ok(parsed) = val.parse::<f64>() {
                                outer_boundary_ratio.set(parsed);
                            }
                        }
                    />
                    <p style="font-size: 11px; color: #666;">"For cycle detection (1.5 = 150% of 2nd largest)"</p>
                </div>

                 <button
                     class="detect-button"
                     on:click=on_detect
                     disabled=move || {
                         loading.get() || match input_type.get() {
                             None => true, // No file uploaded
                             Some(InputType::Json) => lines.get().is_empty(),
                             Some(InputType::Image) => rooms.get().is_empty(), // Images are processed on upload
                             Some(InputType::Svg) => svg_content.get().is_none(),
                         }
                     }
                 >
                     {move || {
                         if loading.get() {
                             "Processing..."
                         } else {
                             match input_type.get() {
                                 None => "Upload a file to begin",
                                 Some(InputType::Json) => "Detect Rooms",
                                 Some(InputType::Image) => "Rooms Detected",
                                 Some(InputType::Svg) => "Detect Rooms from SVG",
                             }
                         }
                     }}
                 </button>
            </div>

            {move || error.get().map(|err| view! {
                <div class="error">
                    {err}
                </div>
            })}


             <div class="stats">
                 {move || match input_type.get() {
                     None => view! {
                         <p>"Upload a file to see stats"</p>
                     }.into_any(),
                     Some(InputType::Json) => view! {
                         <>
                             <p>"Lines: " {move || lines.get().len()}</p>
                             <p>"Rooms Detected: " {move || rooms.get().len()}</p>
                             {move || method_used.get().map(|method| view! {
                                 <p>"Method: " {method}</p>
                             })}
                             {move || execution_time.get().map(|time| view! {
                                 <p>"Time: " {format!("{}ms", time)}</p>
                             })}
                         </>
                     }.into_any(),
                     Some(InputType::Image) => view! {
                         <>
                             <p>"Lines: " {move || lines.get().len()}</p>
                             <p>"Rooms Detected: " {move || rooms.get().len()}</p>
                             {move || method_used.get().map(|method| view! {
                                 <p>"Method: " {method}</p>
                             })}
                             {move || execution_time.get().map(|time| view! {
                                 <p>"Processing Time: " {format!("{}ms", time)}</p>
                             })}
                         </>
                     }.into_any(),
                     Some(InputType::Svg) => view! {
                         <>
                             <p>"Lines: " {move || lines.get().len()}</p>
                             <p>"Rooms Detected: " {move || rooms.get().len()}</p>
                             {move || method_used.get().map(|method| view! {
                                 <p>"Method: " {method}</p>
                             })}
                             {move || execution_time.get().map(|time| view! {
                                 <p>"Processing Time: " {format!("{}ms", time)}</p>
                             })}
                         </>
                     }.into_any(),
                 }}
             </div>

            <div class="canvas-container">
                <canvas
                    node_ref=canvas_ref
                    width="800"
                    height="600"
                    style="border: 1px solid #ccc;"
                />
            </div>

            <div class="rooms-list">
                <h2>"Detected Rooms"</h2>
                <For
                    each=move || rooms.get()
                    key=|room| room.id
                    let:room
                >
                    <div class="room-card">
                        <h3>{format!("Room {}: {}", room.id,
                            room.room_type.as_ref().unwrap_or(&room.name_hint))}</h3>
                        <p>"Area: " {format!("{:.2}", room.area)}</p>
                        <p>"Bounding Box: [{:.1}, {:.1}, {:.1}, {:.1}]"
                            room.bounding_box[0]
                            room.bounding_box[1]
                            room.bounding_box[2]
                            room.bounding_box[3]
                        </p>
                        {room.confidence.map(|conf| view! {
                            <p>"Confidence: " {format!("{:.0}%", conf * 100.0)}</p>
                        })}
                        {(!room.features.is_empty()).then(|| view! {
                            <p>"Features: " {room.features.join(", ")}</p>
                        })}
                        {room.detection_method.as_ref().map(|method| view! {
                            <p style="font-size: 12px; color: #888;">"Detected by: " {method.clone()}</p>
                        })}
                    </div>
                </For>
            </div>
        </div>
    }
}

async fn vectorize_blueprint<T: Serialize>(request: T) -> Result<serde_json::Value, String> {
    let client = reqwest::Client::new();

    let response = client
        .post("http://localhost:3000/vectorize-blueprint")
        .json(&request)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(format!("Server error ({}): {}", status, error_text));
    }

    response
        .json::<serde_json::Value>()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))
}

async fn detect_rooms(request: DetectRequest, strategy: DetectionStrategy) -> Result<DetectResponse, String> {
    let client = reqwest::Client::new();

    // Use different endpoint based on strategy
    let endpoint = match strategy {
        DetectionStrategy::Simple => "http://localhost:3000/detect",
        DetectionStrategy::GraphOnly => "http://localhost:3000/detect",
        _ => "http://localhost:3000/detect", // All strategies use unified endpoint
    };

    let response = client
        .post(endpoint)
        .json(&request)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(format!("Server error ({}): {}", status, error_text));
    }

    response
        .json::<DetectResponse>()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))
}

#[derive(Serialize)]
struct SvgDetectRequest {
    svg_content: String,
    area_threshold: f64,
    door_threshold: f64,
    strategy: String,
    enable_vision: Option<bool>,
}

async fn detect_svg_rooms(svg_content: String, area_threshold: f64, door_threshold: f64, strategy: DetectionStrategy, enable_vision: bool) -> Result<DetectResponse, String> {
    let client = reqwest::Client::new();

    let request = SvgDetectRequest {
        svg_content,
        area_threshold,
        door_threshold,
        strategy: strategy.as_str().to_string(),
        enable_vision: Some(enable_vision),
    };

    let response = client
        .post("http://localhost:3000/detect/svg")
        .json(&request)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(format!("Server error ({}): {}", status, error_text));
    }

    response
        .json::<DetectResponse>()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))
}

// ============================================================================
// DUAL ALGORITHM DETECTOR - Simplified UI for comparing both CC algorithms
// ============================================================================

#[derive(Debug, Clone)]
struct AlgoResult {
    rooms: Vec<Room>,
    execution_time_ms: u64,
    room_count: usize,
}

#[component]
fn DualAlgorithmDetector() -> impl IntoView {
    // State
    let algo1_result = RwSignal::new(Option::<AlgoResult>::None);
    let algo2_result = RwSignal::new(Option::<AlgoResult>::None);
    let loading = RwSignal::new(false);
    let error = RwSignal::new(Option::<String>::None);
    let uploaded_filename = RwSignal::new(Option::<String>::None);
    let use_gpt4o = RwSignal::new(false);
    let gpt4o_result = RwSignal::new(Option::<String>::None);
    let base64_image = RwSignal::new(Option::<String>::None);

    let file_input_ref = NodeRef::<leptos::html::Input>::new();
    let canvas1_ref = NodeRef::<leptos::html::Canvas>::new();
    let canvas2_ref = NodeRef::<leptos::html::Canvas>::new();

    // Re-render canvases when results change
    create_effect(move |_| {
        if let Some(result) = algo1_result.get() {
            if let Some(canvas) = canvas1_ref.get() {
                render_rooms(&canvas, &result.rooms);
            }
        }
    });

    create_effect(move |_| {
        if let Some(result) = algo2_result.get() {
            if let Some(canvas) = canvas2_ref.get() {
                render_rooms(&canvas, &result.rooms);
            }
        }
    });

    // File upload handler
    let on_file_change = move |_| {
        if let Some(input) = file_input_ref.get() {
            if let Some(files) = input.files() {
                if let Some(file) = files.get(0) {
                    let file_name = file.name();
                    let file_type = file.type_();

                    // Only accept PNG/JPG
                    if !file_type.starts_with("image/") ||
                       (!file_name.ends_with(".png") && !file_name.ends_with(".jpg") && !file_name.ends_with(".jpeg")) {
                        error.set(Some("Only PNG and JPG images are supported.".to_string()));
                        return;
                    }

                    uploaded_filename.set(Some(file_name.clone()));
                    error.set(None);
                    loading.set(true);
                    algo1_result.set(None);
                    algo2_result.set(None);
                    gpt4o_result.set(None);

                    // Read as base64
                    let reader = web_sys::FileReader::new().unwrap();
                    let reader_clone = reader.clone();

                    let onload = Closure::wrap(Box::new(move |_: web_sys::Event| {
                        if let Ok(result) = reader_clone.result() {
                            if let Some(data_url) = result.as_string() {
                                let parts: Vec<&str> = data_url.split(",").collect();
                                if parts.len() > 1 {
                                    let b64 = parts[1].to_string();
                                    base64_image.set(Some(b64.clone()));

                                    // Run both algorithms
                                    spawn_local(async move {
                                        let _ = run_dual_detection(
                                            b64,
                                            algo1_result,
                                            algo2_result,
                                            loading,
                                            error,
                                            use_gpt4o,
                                            gpt4o_result,
                                            canvas1_ref,
                                            canvas2_ref,
                                        ).await;
                                    });
                                }
                            }
                        }
                    }) as Box<dyn FnMut(_)>);

                    reader.set_onload(Some(onload.as_ref().unchecked_ref()));
                    onload.forget();
                    let _ = reader.read_as_data_url(&file);
                }
            }
        }
    };

    view! {
        <div class="container" style="max-width: 1600px; margin: 0 auto; padding: 20px;">
            <h1>"Connected Components Room Detector"</h1>
            <p style="color: #666; margin-bottom: 30px;">
                "Dual Algorithm Comparison: Flood Fill vs Connected Components"
            </p>

            <div style="margin-bottom: 20px;">
                <input
                    type="file"
                    accept="image/png,image/jpeg"
                    node_ref=file_input_ref
                    on:change=on_file_change
                    style="padding: 10px;"
                />
                {move || uploaded_filename.get().map(|name| view! {
                    <p style="margin-top: 10px; color: #28a745;">"✓ Uploaded: " {name}</p>
                })}
            </div>

            <div style="margin-bottom: 20px;">
                <label style="cursor: pointer;">
                    <input
                        type="checkbox"
                        prop:checked=move || use_gpt4o.get()
                        on:change=move |ev| use_gpt4o.set(event_target_checked(&ev))
                    />
                    " Enable GPT-4o Validation (experimental)"
                </label>
            </div>

            {move || error.get().map(|err| view! {
                <div style="background: #f8d7da; color: #721c24; padding: 15px; margin-bottom: 20px; border-radius: 5px;">
                    {err}
                </div>
            })}

            {move || if loading.get() {
                view! {
                    <div style="background: #d1ecf1; color: #0c5460; padding: 15px; margin-bottom: 20px; border-radius: 5px;">
                        "⏳ Running both algorithms in parallel..."
                    </div>
                }.into_any()
            } else {
                view! { <div></div> }.into_any()
            }}

            <div style="display: grid; grid-template-columns: 1fr 1fr; gap: 30px; margin-top: 30px;">
                // Algorithm 1
                <div style="border: 2px solid #ddd; border-radius: 8px; padding: 20px;">
                    <h2 style="color: #0056b3;">"Algorithm 1: Flood Fill"</h2>
                    {move || if let Some(result) = algo1_result.get() {
                        view! {
                            <div>
                                <div style="display: flex; gap: 20px; margin: 15px 0;">
                                    <div>
                                        <strong>"Rooms: "</strong>
                                        <span style="font-size: 24px; color: #28a745;">{result.room_count}</span>
                                    </div>
                                    <div>
                                        <strong>"Time: "</strong>
                                        <span style="font-size: 24px; color: #007bff;">{result.execution_time_ms}"ms"</span>
                                    </div>
                                </div>
                                <button
                                    style="margin: 10px 0; padding: 8px 16px; background: #007bff; color: white; border: none; border-radius: 4px; cursor: pointer;"
                                    on:click=move |_| {
                                        if let Some(res) = algo1_result.get() {
                                            download_json(&res.rooms, "algorithm1_flood_fill.json");
                                        }
                                    }
                                >
                                    "Download JSON"
                                </button>
                                <canvas
                                    node_ref=canvas1_ref
                                    width="700"
                                    height="500"
                                    style="border: 1px solid #ccc; width: 100%; max-width: 700px;"
                                />
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <div style="padding: 40px; text-align: center; color: #999;">
                                "Upload an image to see results"
                            </div>
                        }.into_any()
                    }}
                </div>

                // Algorithm 2
                <div style="border: 2px solid #ddd; border-radius: 8px; padding: 20px;">
                    <h2 style="color: #0056b3;">"Algorithm 2: Connected Components"</h2>
                    {move || if let Some(result) = algo2_result.get() {
                        view! {
                            <div>
                                <div style="display: flex; gap: 20px; margin: 15px 0;">
                                    <div>
                                        <strong>"Rooms: "</strong>
                                        <span style="font-size: 24px; color: #28a745;">{result.room_count}</span>
                                    </div>
                                    <div>
                                        <strong>"Time: "</strong>
                                        <span style="font-size: 24px; color: #007bff;">{result.execution_time_ms}"ms"</span>
                                    </div>
                                </div>
                                <button
                                    style="margin: 10px 0; padding: 8px 16px; background: #007bff; color: white; border: none; border-radius: 4px; cursor: pointer;"
                                    on:click=move |_| {
                                        if let Some(res) = algo2_result.get() {
                                            download_json(&res.rooms, "algorithm2_connected_components.json");
                                        }
                                    }
                                >
                                    "Download JSON"
                                </button>
                                <canvas
                                    node_ref=canvas2_ref
                                    width="700"
                                    height="500"
                                    style="border: 1px solid #ccc; width: 100%; max-width: 700px;"
                                />
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <div style="padding: 40px; text-align: center; color: #999;">
                                "Upload an image to see results"
                            </div>
                        }.into_any()
                    }}
                </div>
            </div>

            {move || if use_gpt4o.get() {
                view! {
                    <div style="margin-top: 30px; padding: 20px; background: #f8f9fa; border-radius: 8px;">
                        <h2>"GPT-4o Validation"</h2>
                        {move || if let Some(validation) = gpt4o_result.get() {
                            view! {
                                <p>{validation}</p>
                            }.into_any()
                        } else if loading.get() {
                            view! {
                                <p style="color: #666;">"Waiting for validation..."</p>
                            }.into_any()
                        } else {
                            view! {
                                <p style="color: #999;">"Upload an image to see validation"</p>
                            }.into_any()
                        }}
                    </div>
                }.into_any()
            } else {
                view! { <div></div> }.into_any()
            }}
        </div>
    }
}

async fn run_dual_detection(
    base64_img: String,
    algo1: RwSignal<Option<AlgoResult>>,
    algo2: RwSignal<Option<AlgoResult>>,
    loading: RwSignal<bool>,
    error: RwSignal<Option<String>>,
    use_gpt4o: RwSignal<bool>,
    gpt4o_result: RwSignal<Option<String>>,
    canvas1: NodeRef<leptos::html::Canvas>,
    canvas2: NodeRef<leptos::html::Canvas>,
) -> Result<(), gloo_net::Error> {
    use gloo_net::http::Request;
    use serde_json::json;

    let payload = json!({
        "image": base64_img,
        "threshold": 127,
        "min_area": 500,
        "max_area_ratio": 0.3
    });

    // Algorithm 1: Flood Fill
    let start1 = Date::now();
    match Request::post("http://localhost:3000/detect/rust-floodfill")
        .header("Content-Type", "application/json")
        .body(payload.to_string())?
        .send()
        .await
    {
        Ok(resp) => {
            let time1 = (Date::now() - start1) as u64;
            match resp.json::<serde_json::Value>().await {
                Ok(data) => {
                    if let Some(rooms_arr) = data.get("rooms").and_then(|v| v.as_array()) {
                        let rooms: Vec<Room> = serde_json::from_value(serde_json::Value::Array(rooms_arr.clone())).unwrap_or_default();
                        let count = data.get("total_rooms").and_then(|v| v.as_u64()).unwrap_or(rooms.len() as u64) as usize;

                        algo1.set(Some(AlgoResult {
                            rooms: rooms.clone(),
                            execution_time_ms: time1,
                            room_count: count,
                        }));

                        // Render
                        if let Some(canvas) = canvas1.get() {
                            render_rooms(&canvas, &rooms);
                        }
                    }
                }
                Err(e) => error.set(Some(format!("Algo 1 parse error: {}", e))),
            }
        }
        Err(e) => error.set(Some(format!("Algo 1 request failed: {}", e))),
    }

    // Algorithm 2: Connected Components
    let start2 = Date::now();
    match Request::post("http://localhost:3000/detect/connected-components")
        .header("Content-Type", "application/json")
        .body(payload.to_string())?
        .send()
        .await
    {
        Ok(resp) => {
            let time2 = (Date::now() - start2) as u64;
            match resp.json::<serde_json::Value>().await {
                Ok(data) => {
                    if let Some(rooms_arr) = data.get("rooms").and_then(|v| v.as_array()) {
                        let rooms: Vec<Room> = serde_json::from_value(serde_json::Value::Array(rooms_arr.clone())).unwrap_or_default();
                        let count = data.get("total_rooms").and_then(|v| v.as_u64()).unwrap_or(rooms.len() as u64) as usize;

                        algo2.set(Some(AlgoResult {
                            rooms: rooms.clone(),
                            execution_time_ms: time2,
                            room_count: count,
                        }));

                        // Render
                        if let Some(canvas) = canvas2.get() {
                            render_rooms(&canvas, &rooms);
                        }
                    }
                }
                Err(e) => error.set(Some(format!("Algo 2 parse error: {}", e))),
            }
        }
        Err(e) => error.set(Some(format!("Algo 2 request failed: {}", e))),
    }

    // GPT-4o validation
    if use_gpt4o.get() {
        gpt4o_result.set(Some("🔄 Running GPT-4o validation...".to_string()));

        match run_gpt4o_validation(
            &base64_img,
            algo1.get().as_ref(),
            algo2.get().as_ref(),
        ).await {
            Ok(validation_text) => {
                gpt4o_result.set(Some(validation_text));
            }
            Err(e) => {
                gpt4o_result.set(Some(format!("❌ GPT-4o validation failed: {:?}", e)));
            }
        }
    }

    loading.set(false);
    Ok(())
}

async fn run_gpt4o_validation(
    base64_image: &str,
    algo1_result: Option<&AlgoResult>,
    algo2_result: Option<&AlgoResult>,
) -> Result<String, gloo_net::Error> {
    use gloo_net::http::Request;
    use serde_json::json;

    // Prepare the validation prompt
    let algo1_count = algo1_result.map(|r| r.room_count).unwrap_or(0);
    let algo2_count = algo2_result.map(|r| r.room_count).unwrap_or(0);

    let prompt = format!(
        "You are analyzing a blueprint/floorplan image. Two different room detection algorithms have been run:\n\n\
        - Algorithm 1 (Flood Fill): Detected {} rooms\n\
        - Algorithm 2 (Connected Components): Detected {} rooms\n\n\
        Please analyze this blueprint image and:\n\
        1. Count the actual number of rooms visible in the image\n\
        2. Evaluate whether the algorithm results are accurate\n\
        3. Identify any rooms that might have been missed or incorrectly detected\n\
        4. Provide a confidence score (0-100%) for each algorithm\n\
        5. Give your final recommendation on which algorithm performed better\n\n\
        Be specific and reference visual elements from the image in your analysis.",
        algo1_count, algo2_count
    );

    let request_body = json!({
        "model": "gpt-4o",
        "messages": [
            {
                "role": "user",
                "content": [
                    {
                        "type": "text",
                        "text": prompt
                    },
                    {
                        "type": "image_url",
                        "image_url": {
                            "url": format!("data:image/png;base64,{}", base64_image)
                        }
                    }
                ]
            }
        ],
        "max_tokens": 1000
    });

    // Call backend endpoint for GPT-4o
    match Request::post("http://localhost:3000/validate/gpt4o")
        .header("Content-Type", "application/json")
        .body(request_body.to_string())?
        .send()
        .await
    {
        Ok(response) => {
            if response.ok() {
                match response.json::<serde_json::Value>().await {
                    Ok(data) => {
                        if let Some(content) = data
                            .get("choices")
                            .and_then(|c| c.get(0))
                            .and_then(|c| c.get("message"))
                            .and_then(|m| m.get("content"))
                            .and_then(|c| c.as_str())
                        {
                            Ok(format!("✅ GPT-4o Analysis:\n\n{}", content))
                        } else {
                            Err(gloo_net::Error::GlooError("Invalid response format from GPT-4o".to_string()))
                        }
                    }
                    Err(e) => Err(e),
                }
            } else {
                Err(gloo_net::Error::GlooError(format!("GPT-4o API returned status: {}", response.status())))
            }
        }
        Err(e) => Err(e),
    }
}

fn download_json(rooms: &[Room], filename: &str) {
    use wasm_bindgen::JsValue;

    let json_str = serde_json::to_string_pretty(rooms).unwrap_or_else(|_| "{}".to_string());

    // Create a blob
    let array = js_sys::Array::new();
    array.push(&JsValue::from_str(&json_str));

    let blob = web_sys::Blob::new_with_str_sequence(&array).unwrap();
    let url = web_sys::Url::create_object_url_with_blob(&blob).unwrap();

    // Create download link and trigger
    let document = web_sys::window().unwrap().document().unwrap();
    let a = document.create_element("a").unwrap();
    let a = a.dyn_into::<web_sys::HtmlAnchorElement>().unwrap();

    a.set_href(&url);
    a.set_download(filename);
    a.click();

    // Cleanup
    web_sys::Url::revoke_object_url(&url).unwrap();
}

fn render_rooms(canvas: &web_sys::HtmlCanvasElement, rooms: &[Room]) {
    let ctx = canvas
        .get_context("2d")
        .unwrap()
        .unwrap()
        .dyn_into::<web_sys::CanvasRenderingContext2d>()
        .unwrap();

    let w = canvas.width() as f64;
    let h = canvas.height() as f64;

    // Clear
    ctx.clear_rect(0.0, 0.0, w, h);
    ctx.set_fill_style(&"white".into());
    ctx.fill_rect(0.0, 0.0, w, h);

    // Draw rooms
    let colors = ["#FF6B6B", "#4ECDC4", "#45B7D1", "#FFA07A", "#98D8C8", "#F7DC6F", "#BB8FCE", "#85C1E2", "#F8B88B", "#AAB7B8"];

    for (i, room) in rooms.iter().enumerate() {
        if room.bounding_box.len() >= 4 {
            let color = colors[i % colors.len()];
            ctx.set_fill_style(&color.into());
            ctx.set_stroke_style(&"#333".into());
            ctx.set_line_width(2.0);

            let x1 = (room.bounding_box[0] / 1000.0) * w;
            let y1 = (room.bounding_box[1] / 1000.0) * h;
            let x2 = (room.bounding_box[2] / 1000.0) * w;
            let y2 = (room.bounding_box[3] / 1000.0) * h;

            let rw = x2 - x1;
            let rh = y2 - y1;

            ctx.fill_rect(x1, y1, rw, rh);
            ctx.stroke_rect(x1, y1, rw, rh);

            ctx.set_fill_style(&"#000".into());
            ctx.set_font("14px Arial");
            let _ = ctx.fill_text(&format!("Room {}", room.id), x1 + 10.0, y1 + 25.0);
        }
    }
}

#[component]
fn AlgorithmTest() -> impl IntoView {
    let image_content = RwSignal::new(Option::<String>::None); // Store base64 image

    // Room storage for all 7 algorithms
    let simple_rooms = RwSignal::new(Vec::<Room>::new());
    let graph_rooms = RwSignal::new(Vec::<Room>::new());
    let original_cc_rooms = RwSignal::new(Vec::<Room>::new());
    let rust_floodfill_rooms = RwSignal::new(Vec::<Room>::new());
    let vector_graph_rooms = RwSignal::new(Vec::<Room>::new());
    let python_cc_rooms = RwSignal::new(Vec::<Room>::new());
    let graph_image_rooms = RwSignal::new(Vec::<Room>::new());

    // Line storage for all 7 algorithms
    let simple_lines = RwSignal::new(Vec::<Line>::new());
    let graph_lines = RwSignal::new(Vec::<Line>::new());
    let original_cc_lines = RwSignal::new(Vec::<Line>::new());
    let rust_floodfill_lines = RwSignal::new(Vec::<Line>::new());
    let vector_graph_lines = RwSignal::new(Vec::<Line>::new());
    let python_cc_lines = RwSignal::new(Vec::<Line>::new());
    let graph_image_lines = RwSignal::new(Vec::<Line>::new());

    let loading = RwSignal::new(false);
    let error = RwSignal::new(Option::<String>::None);
    let area_threshold = RwSignal::new(100.0);
    let door_threshold = RwSignal::new(50.0);
    let coverage_threshold = RwSignal::new(0.3);

    // Canvas refs for all 7 algorithms
    let file_input_ref = NodeRef::<leptos::html::Input>::new();
    let canvas_simple_ref = NodeRef::<leptos::html::Canvas>::new();
    let canvas_graph_ref = NodeRef::<leptos::html::Canvas>::new();
    let canvas_original_cc_ref = NodeRef::<leptos::html::Canvas>::new();
    let canvas_rust_floodfill_ref = NodeRef::<leptos::html::Canvas>::new();
    let canvas_vector_graph_ref = NodeRef::<leptos::html::Canvas>::new();
    let canvas_python_cc_ref = NodeRef::<leptos::html::Canvas>::new();
    let canvas_graph_image_ref = NodeRef::<leptos::html::Canvas>::new();

    // Handle JSON or image file upload
    let on_file_change = move |_| {
        if let Some(input) = file_input_ref.get() {
            if let Some(files) = input.files() {
                if let Some(file) = files.get(0) {
                    let file_name = file.name();
                    let file_type = file.type_();

                    // Check if it's a JSON file
                    if file_name.ends_with(".json") || file_type == "application/json" {
                        let reader = web_sys::FileReader::new().unwrap();
                        let reader_clone = reader.clone();

                        let onload = Closure::wrap(Box::new(move |_event: web_sys::Event| {
                            if let Ok(result) = reader_clone.result() {
                                if let Some(text) = result.as_string() {
                                    // Parse JSON lines directly
                                    match serde_json::from_str::<Vec<Line>>(&text) {
                                        Ok(lines) => {
                                            simple_lines.set(lines.clone());
                                            graph_lines.set(lines);
                                            image_content.set(Some("json_loaded".to_string()));
                                            error.set(None);
                                        }
                                        Err(e) => {
                                            error.set(Some(format!("Invalid JSON format: {}", e)));
                                        }
                                    }
                                }
                            }
                        }) as Box<dyn FnMut(_)>);

                        reader.set_onload(Some(onload.as_ref().unchecked_ref()));
                        onload.forget();
                        let _ = reader.read_as_text(&file);
                    } else if file_type.starts_with("image/") {
                        // Handle image upload
                        let reader = web_sys::FileReader::new().unwrap();
                        let reader_clone = reader.clone();

                        let onload = Closure::wrap(Box::new(move |_event: web_sys::Event| {
                            if let Ok(result) = reader_clone.result() {
                                if let Some(data_url) = result.as_string() {
                                    // Extract base64 data
                                    if let Some(base64_data) = data_url.split(',').nth(1) {
                                        image_content.set(Some(base64_data.to_string()));
                                        error.set(None);
                                    }
                                }
                            }
                        }) as Box<dyn FnMut(_)>);

                        reader.set_onload(Some(onload.as_ref().unchecked_ref()));
                        onload.forget();
                        let _ = reader.read_as_data_url(&file);
                    } else {
                        error.set(Some("Please upload an image file (PNG, JPG, SVG) or JSON file".to_string()));
                    }
                }
            }
        }
    };

    // Run both algorithms
let on_detect = move |_| {
    let Some(img_base64) = image_content.get() else {
        error.set(Some("Please upload a file first".to_string()));
        return;
    };

    loading.set(true);
    error.set(None);

    let current_area = area_threshold.get();
    let current_door = door_threshold.get();
    let current_coverage = coverage_threshold.get();

    // Check if JSON was loaded (lines already parsed)
    if img_base64 == "json_loaded" {
        // Lines are already loaded, run algorithms directly
        let lines_for_simple = simple_lines.get();
        let lines_for_graph = graph_lines.get();

        // Step 1: Run Simple algorithm
        spawn_local({
            let lines = lines_for_simple.clone();
            let area = current_area;
            let coverage = current_coverage;
            async move {
                let request = DetectRequest {
                    lines,
                    area_threshold: area,
                    door_threshold: None,
                    coverage_threshold: Some(coverage),
                    outer_boundary_ratio: None,
                    strategy: "Simple".to_string(),
                    enable_vision: Some(false),
                    enable_yolo: Some(false),
                };

                match detect_rooms(request, DetectionStrategy::Simple).await {
                    Ok(response) => {
                        simple_rooms.set(response.rooms);
                    }
                    Err(e) => {
                        error.set(Some(format!("Simple algorithm failed: {}", e)));
                    }
                }
            }
        });

        // Step 2: Run GraphOnly algorithm
        spawn_local({
            let lines = lines_for_graph.clone();
            let area = current_area;
            let door = current_door;
            async move {
                let request = DetectRequest {
                    lines,
                    area_threshold: area,
                    door_threshold: Some(door),
                    coverage_threshold: None,
                    outer_boundary_ratio: Some(1.5),
                    strategy: "GraphOnly".to_string(),
                    enable_vision: Some(false),
                    enable_yolo: Some(false),
                };

                match detect_rooms(request, DetectionStrategy::GraphOnly).await {
                    Ok(response) => {
                        graph_rooms.set(response.rooms);
                    }
                    Err(e) => {
                        error.set(Some(format!("GraphOnly algorithm failed: {}", e)));
                    }
                }
            }
        });
    } else {
        // It's an image, run all 4 algorithms in parallel
        let img_for_all = img_base64.clone();
        let area = current_area;
        let threshold = 128u8;
        let min_area = 500usize;
        let max_area_ratio = 0.3f32;

        // 6. Original Connected Components - ACTIVE (Display Algorithm 6)
        spawn_local({
            let img = img_for_all.clone();
            async move {
                match detect_original_cc(img).await {
                    Ok(response) => {
                        original_cc_rooms.set(response.rooms);
                    }
                    Err(e) => {
                        error.set(Some(format!("Original CC failed: {}", e)));
                    }
                }
            }
        });

        // 3. Rust Flood Fill - ACTIVE (Display Algorithm 3)
        spawn_local({
            let img = img_for_all.clone();
            async move {
                match detect_rust_floodfill(img, threshold, min_area, max_area_ratio).await {
                    Ok(response) => {
                        rust_floodfill_rooms.set(response.rooms);
                        loading.set(false);
                    }
                    Err(e) => {
                        error.set(Some(format!("Rust Flood Fill failed: {}", e)));
                        loading.set(false);
                    }
                }
            }
        });
    }
};

    // Render canvases
Effect::new(move |_| {
    if let Some(canvas) = canvas_simple_ref.get_untracked() {
        render_floorplan(&canvas, &simple_lines.get(), &simple_rooms.get());
    }
});

Effect::new(move |_| {
    if let Some(canvas) = canvas_graph_ref.get_untracked() {
        render_floorplan(&canvas, &graph_lines.get(), &graph_rooms.get());
    }
});

Effect::new(move |_| {
    if let Some(canvas) = canvas_rust_floodfill_ref.get_untracked() {
        render_floorplan(&canvas, &rust_floodfill_lines.get(), &rust_floodfill_rooms.get());
    }
});

Effect::new(move |_| {
    if let Some(canvas) = canvas_vector_graph_ref.get_untracked() {
        render_floorplan(&canvas, &vector_graph_lines.get(), &vector_graph_rooms.get());
    }
});

Effect::new(move |_| {
    if let Some(canvas) = canvas_rust_floodfill_ref.get_untracked() {
        render_floorplan(&canvas, &rust_floodfill_lines.get(), &rust_floodfill_rooms.get());
    }
});

Effect::new(move |_| {
    if let Some(canvas) = canvas_vector_graph_ref.get_untracked() {
        render_floorplan(&canvas, &vector_graph_lines.get(), &vector_graph_rooms.get());
    }
});

Effect::new(move |_| {
    if let Some(canvas) = canvas_rust_floodfill_ref.get_untracked() {
        render_floorplan(&canvas, &rust_floodfill_lines.get(), &rust_floodfill_rooms.get());
    }
});

Effect::new(move |_| {
    if let Some(canvas) = canvas_vector_graph_ref.get_untracked() {
        render_floorplan(&canvas, &vector_graph_lines.get(), &vector_graph_rooms.get());
    }
});

Effect::new(move |_| {
    if let Some(canvas) = canvas_rust_floodfill_ref.get_untracked() {
        render_floorplan(&canvas, &rust_floodfill_lines.get(), &rust_floodfill_rooms.get());
    }
});

Effect::new(move |_| {
    if let Some(canvas) = canvas_vector_graph_ref.get_untracked() {
        render_floorplan(&canvas, &vector_graph_lines.get(), &vector_graph_rooms.get());
    }
});

Effect::new(move |_| {
    if let Some(canvas) = canvas_python_cc_ref.get_untracked() {
        render_floorplan(&canvas, &python_cc_lines.get(), &python_cc_rooms.get());
    }
});

Effect::new(move |_| {
    if let Some(canvas) = canvas_original_cc_ref.get_untracked() {
        render_floorplan(&canvas, &simple_lines.get(), &original_cc_rooms.get());
    }
});

Effect::new(move |_| {
    if let Some(canvas) = canvas_graph_image_ref.get_untracked() {
        render_floorplan(&canvas, &graph_lines.get(), &graph_image_rooms.get());
    }
});

Effect::new(move |_| {
    if let Some(canvas) = canvas_rust_floodfill_ref.get_untracked() {
        render_floorplan(&canvas, &rust_floodfill_lines.get(), &rust_floodfill_rooms.get());
    }
});

Effect::new(move |_| {
    if let Some(canvas) = canvas_vector_graph_ref.get_untracked() {
        render_floorplan(&canvas, &vector_graph_lines.get(), &vector_graph_rooms.get());
    }
});

Effect::new(move |_| {
    if let Some(canvas) = canvas_python_cc_ref.get_untracked() {
        render_floorplan(&canvas, &python_cc_lines.get(), &python_cc_rooms.get());
    }
});

Effect::new(move |_| {
    if let Some(canvas) = canvas_original_cc_ref.get_untracked() {
        render_floorplan(&canvas, &simple_lines.get(), &original_cc_rooms.get());
    }
});

Effect::new(move |_| {
    if let Some(canvas) = canvas_graph_image_ref.get_untracked() {
        render_floorplan(&canvas, &graph_lines.get(), &graph_image_rooms.get());
    }
});

    view! {
        <div class="container">
            <header>
                <h1>"Room Detection Algorithm Comparison"</h1>
                <p>"Upload an image/SVG to compare Simple vs GraphOnly algorithms"</p>
                <a href="/" style="color: #007bff; text-decoration: none;">"← Back to Main UI"</a>
            </header>

            <div class="controls">
                <div class="file-upload-primary">
                    <h3>"Upload Image, SVG, or JSON"</h3>
                    <input
                        type="file"
                        accept=".svg,.png,.jpg,.jpeg,.json"
                        node_ref=file_input_ref
                        on:change=on_file_change
                    />
                    <p style="font-size: 12px; color: #666; margin-top: 5px;">
                        "Tip: Upload a JSON file from test-data/ for instant results!"
                    </p>
                </div>

                <div class="threshold-control">
                    <label for="threshold">"Area Threshold:"</label>
                    <input
                        type="number"
                        id="threshold"
                        prop:value=move || area_threshold.get()
                        on:input=move |ev| {
                            let val = ev.target().unwrap().unchecked_into::<web_sys::HtmlInputElement>().value();
                            if let Ok(parsed) = val.parse::<f64>() {
                                area_threshold.set(parsed);
                            }
                        }
                    />
                </div>

                <div class="threshold-control">
                    <label for="door-threshold">"Door Threshold:"</label>
                    <input
                        type="number"
                        id="door-threshold"
                        prop:value=move || door_threshold.get()
                        on:input=move |ev| {
                            let val = ev.target().unwrap().unchecked_into::<web_sys::HtmlInputElement>().value();
                            if let Ok(parsed) = val.parse::<f64>() {
                                door_threshold.set(parsed);
                            }
                        }
                    />
                </div>

                <div class="threshold-control">
                    <label for="coverage-threshold">"Coverage Threshold:"</label>
                    <input
                        type="number"
                        id="coverage-threshold"
                        step="0.1"
                        prop:value=move || coverage_threshold.get()
                        on:input=move |ev| {
                            let val = ev.target().unwrap().unchecked_into::<web_sys::HtmlInputElement>().value();
                            if let Ok(parsed) = val.parse::<f64>() {
                                coverage_threshold.set(parsed);
                            }
                        }
                    />
                </div>

                <button
                    class="detect-button"
                    on:click=on_detect
                    disabled=move || loading.get() || image_content.get().is_none()
                >
                    {move || if loading.get() { "Processing..." } else { "Run Both Algorithms" }}
                </button>
            </div>

            {move || error.get().map(|err| view! {
                <div class="error">{err}</div>
            })}

<div style="display: grid; grid-template-columns: repeat(auto-fit, minmax(300px, 1fr)); gap: 20px; margin-top: 20px;">
    <div>
        <h3>"1. Enhanced Flood Fill (Aggressive)"</h3>
        <p style="font-size: 12px; color: #666; line-height: 1.4;">
            "Optimized connected components with 8-connectivity flood fill. Uses threshold=140, min_area=500, max_area=1/5 of image, aspect_ratio<25. More aggressive filtering to detect smaller rooms and complex spaces."
        </p>
        <canvas
            node_ref=canvas_rust_floodfill_ref
            width="400"
            height="300"
            style="border: 1px solid #ccc; width: 100%; height: auto;"
        />
        <div class="stats">
            <p>"Rooms: " {move || rust_floodfill_rooms.get().len()}</p>
        </div>
    </div>

    <div>
        <h3>"2. Baseline CC (Conservative)"</h3>
        <p style="font-size: 12px; color: #666; line-height: 1.4;">
            "Standard connected components with 8-connectivity flood fill. Uses threshold=140, min_area=250, max_area=35% of image, aspect_ratio<15. More conservative filtering for higher confidence room detection."
        </p>
        <canvas
            node_ref=canvas_original_cc_ref
            width="400"
            height="300"
            style="border: 1px solid #ccc; width: 100%; height: auto;"
        />
        <div class="stats">
            <p>"Rooms: " {move || original_cc_rooms.get().len()}</p>
        </div>
    </div>
</div>
        </div>
    }
}


async fn detect_original_cc(image: String) -> Result<DetectResponse, String> {
    let client = reqwest::Client::new();

    #[derive(Serialize)]
    struct ImageDetectRequest {
        image: String,
        threshold: u8,
        min_area: usize,
        max_area_ratio: f32,
    }

    let request = ImageDetectRequest {
        image,
        threshold: 140,
        min_area: 250,
        max_area_ratio: 0.35,
    };

    let response = client
        .post("http://localhost:3000/detect/connected-components")
        .json(&request)
        .send()
        .await
        .map_err(|e| format!("Original CC failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(format!("Server error ({}): {}", status, error_text));
    }

    // Parse the backend response which has {rooms, total_rooms}
    #[derive(Deserialize)]
    struct BackendResponse {
        rooms: Vec<Room>,
        total_rooms: usize,
    }

    let backend_response: BackendResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    // Convert to frontend DetectResponse format
    Ok(DetectResponse {
        rooms: backend_response.rooms,
        method_used: Some("connected-components".to_string()),
        execution_time_ms: None,
        metadata: None,
    })
}

async fn detect_rust_floodfill(
    image: String,
    threshold: u8,
    min_area: usize,
    max_area_ratio: f32,
) -> Result<DetectResponse, String> {
    let client = reqwest::Client::new();

    #[derive(Serialize)]
    struct ImageDetectRequest {
        image: String,
        threshold: u8,
        min_area: usize,
        max_area_ratio: f32,
    }

    let request = ImageDetectRequest {
        image,
        threshold,
        min_area,
        max_area_ratio,
    };

    let response = client
        .post("http://localhost:3000/detect/rust-floodfill")
        .json(&request)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(format!("Server error ({}): {}", status, error_text));
    }

    // Parse the backend response which has {rooms, total_rooms}
    #[derive(Deserialize)]
    struct BackendResponse {
        rooms: Vec<Room>,
        total_rooms: usize,
    }

    let backend_response: BackendResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    // Convert to frontend DetectResponse format
    Ok(DetectResponse {
        rooms: backend_response.rooms,
        method_used: Some("rust-floodfill".to_string()),
        execution_time_ms: None,
        metadata: None,
    })
}


#[wasm_bindgen(start)]
pub fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App);
}
