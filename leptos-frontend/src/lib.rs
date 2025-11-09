use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_meta::*;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

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
    SvgWithAiParser,
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
            Self::SvgWithAiParser => "SvgWithAiParser",
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
            Self::SvgWithAiParser => "SVG AI Parser - Use AI models to interpret SVG instead of geometric parsing",
        }
    }

    fn input_type(&self) -> InputType {
        match self {
            Self::Simple | Self::GraphOnly | Self::GraphWithVision | Self::YoloOnly | Self::BestAvailable | Self::Ensemble => InputType::Json,
            Self::HybridVision | Self::VTracerOnly | Self::Gpt5Only => InputType::Image,
            Self::SvgOnly | Self::SvgWithVision | Self::SvgWithAiParser => InputType::Svg,
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
        <FloorplanDetector/>
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
                    let backend_strategy = "graph_only".to_string();

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
                    let backend_strategy = match current_svg_parser {
                        SvgParser::Algorithmic => DetectionStrategy::SvgOnly,
                        SvgParser::Gpt5Nano => DetectionStrategy::SvgWithAiParser,
                        SvgParser::Combined => DetectionStrategy::SvgWithAiParser, // TODO: Add Combined support to backend
                    };

                    let enable_vision = false; // SVG doesn't use vision in current architecture

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
                                            <small>"🧠 AI understanding • ⏱️ Variable time"</small>
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
                                            <small>"🔬 Validation • ⏱️ 2x time"</small>
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
                             <p>"Lines: " {0}</p>
                             <p>"Rooms Detected: " {move || rooms.get().len()}</p>
                             <p>"Method: SVG Processing"</p>
                             <p>"Time: Coming Soon"</p>
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
        DetectionStrategy::Simple => "http://localhost:3000/detect/simple",
        DetectionStrategy::GraphOnly => "http://localhost:3000/detect",
        _ => "http://localhost:3000/detect/enhanced", // GraphWithVision, BestAvailable, Ensemble
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

#[wasm_bindgen(start)]
pub fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App);
}
