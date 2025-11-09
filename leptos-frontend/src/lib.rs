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
    let strategy = RwSignal::new(DetectionStrategy::HybridVision);
    let input_type = RwSignal::new(InputType::Image); // Default to image for backward compatibility
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
                    let current_input_type = input_type.get();

                    match current_input_type {
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

    let current_strategy = strategy.get();

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

                        let request = VectorizeRequest {
                            image: base64_owned,
                            strategy: current_strategy.as_str().to_string(),
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
        let current_input_type = input_type.get();
        let current_strategy = strategy.get();

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

                    let request = DetectRequest {
                        lines: current_lines,
                        area_threshold: current_area_threshold,
                        door_threshold: Some(current_door_threshold),
                        coverage_threshold: Some(current_coverage_threshold),
                        outer_boundary_ratio: Some(current_outer_boundary_ratio),
                        strategy: current_strategy.as_str().to_string(),
                        enable_vision: Some(current_strategy == DetectionStrategy::GraphWithVision),
                        enable_yolo: Some(current_strategy == DetectionStrategy::YoloOnly),
                    };

                    match detect_rooms(request, current_strategy).await {
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
                    let current_enable_vision = matches!(current_strategy, DetectionStrategy::SvgWithVision);

                    match detect_svg_rooms(current_svg, current_area_threshold, current_door_threshold, current_strategy, current_enable_vision).await {
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
                // Step 1: Choose Input Type
                <div class="input-type-selector">
                    <h3>"Step 1: Choose Input Type"</h3>
                    <div class="input-type-options">
                        <label class="input-type-option">
                            <input
                                type="radio"
                                name="input-type"
                                value="json"
                                checked=move || matches!(input_type.get(), InputType::Json)
                                on:change=move |_| {
                                    input_type.set(InputType::Json);
                                    strategy.set(DetectionStrategy::GraphOnly); // Default for JSON
                                }
                            />
                            <div class="option-content">
                                <strong>"JSON Lines"</strong>
                                <p>"Upload pre-processed line data from floorplan parsing tools"</p>
                                <small>"Best for: Custom parsers, existing line data"</small>
                            </div>
                        </label>

                        <label class="input-type-option">
                            <input
                                type="radio"
                                name="input-type"
                                value="image"
                                checked=move || matches!(input_type.get(), InputType::Image)
                                on:change=move |_| {
                                    input_type.set(InputType::Image);
                                    strategy.set(DetectionStrategy::HybridVision); // Default for images
                                }
                            />
                            <div class="option-content">
                                <strong>"Blueprint Image"</strong>
                                <p>"Upload scanned blueprints (PNG/JPG) for AI processing"</p>
                                <small>"Best for: Scanned documents, photos of floorplans"</small>
                            </div>
                        </label>

                        <label class="input-type-option">
                            <input
                                type="radio"
                                name="input-type"
                                value="svg"
                                checked=move || matches!(input_type.get(), InputType::Svg)
                                on:change=move |_| {
                                    input_type.set(InputType::Svg);
                                    strategy.set(DetectionStrategy::SvgOnly); // Default for SVG
                                }
                            />
                            <div class="option-content">
                                <strong>"Vector SVG"</strong>
                                <p>"Upload clean vector floorplans (SVG format)"</p>
                                <small>"Best for: CAD exports, vector drawings, fast processing"</small>
                            </div>
                        </label>
                    </div>
                </div>

                // Step 2: Choose Detection Strategy
                <div class="strategy-selector">
                    <h3>"Step 2: Choose Detection Method"</h3>
                    <div class="strategy-options">
                         {move || {
                             let current_input = input_type.get();
                             match current_input {
                                 InputType::Json => view! {
                                     <div class="strategy-group">
                                         <h4>"JSON-Based Detection"</h4>
                                         <label class="strategy-option">
                                             <input
                                                 type="radio"
                                                 name="strategy"
                                                 value="Simple"
                                                 checked=move || matches!(strategy.get(), DetectionStrategy::Simple)
                                                 on:change=move |_| strategy.set(DetectionStrategy::Simple)
                                             />
                                             <div class="option-content">
                                                 <strong>"Simple Divider Detection"</strong>
                                                 <p>"Fast geometric analysis for rectangular layouts with vertical dividers"</p>
                                                 <small>"⚡ Fast (~1ms) • 📐 Basic accuracy"</small>
                                             </div>
                                         </label>

                                         <label class="strategy-option">
                                             <input
                                                 type="radio"
                                                 name="strategy"
                                                 value="GraphOnly"
                                                 checked=move || matches!(strategy.get(), DetectionStrategy::GraphOnly)
                                                 on:change=move |_| strategy.set(DetectionStrategy::GraphOnly)
                                             />
                                             <div class="option-content">
                                                 <strong>"Graph Cycle Detection"</strong>
                                                 <p>"Advanced geometric analysis for complex polygons (L-shapes, pentagons)"</p>
                                                 <small>"🔍 Accurate • 🕐 Fast (~10ms)"</small>
                                             </div>
                                         </label>

                                         <label class="strategy-option">
                                             <input
                                                 type="radio"
                                                 name="strategy"
                                                 value="GraphWithVision"
                                                 checked=move || matches!(strategy.get(), DetectionStrategy::GraphWithVision)
                                                 on:change=move |_| strategy.set(DetectionStrategy::GraphWithVision)
                                             />
                                             <div class="option-content">
                                                 <strong>"Graph + AI Classification"</strong>
                                                 <p>"Geometric detection enhanced with AI room type classification"</p>
                                                 <small>"🎯 High accuracy • ⏱️ Slower (~54s)"</small>
                                             </div>
                                         </label>
                                     </div>
                                 }.into_any(),
                                 InputType::Image => view! {
                                     <div class="strategy-group">
                                         <h4>"Image-Based Detection"</h4>
                                         <label class="strategy-option">
                                             <input
                                                 type="radio"
                                                 name="strategy"
                                                 value="HybridVision"
                                                 checked=move || matches!(strategy.get(), DetectionStrategy::HybridVision)
                                                 on:change=move |_| strategy.set(DetectionStrategy::HybridVision)
                                             />
                                             <div class="option-content">
                                                 <strong>"Hybrid Vision Pipeline"</strong>
                                                 <p>"VTracer vectorization + AI vision for comprehensive blueprint analysis"</p>
                                                 <small>"🎯 Best accuracy • ⏱️ Moderate time (~2-3min)"</small>
                                             </div>
                                         </label>

                                         <label class="strategy-option">
                                             <input
                                                 type="radio"
                                                 name="strategy"
                                                 value="VTracerOnly"
                                                 checked=move || matches!(strategy.get(), DetectionStrategy::VTracerOnly)
                                                 on:change=move |_| strategy.set(DetectionStrategy::VTracerOnly)
                                             />
                                             <div class="option-content">
                                                 <strong>"VTracer Vectorization"</strong>
                                                 <p>"Pure geometric vectorization without AI classification"</p>
                                                 <small>"⚡ Fast • 📐 Geometric only"</small>
                                             </div>
                                         </label>

                                         <label class="strategy-option">
                                             <input
                                                 type="radio"
                                                 name="strategy"
                                                 value="Gpt5Only"
                                                 checked=move || matches!(strategy.get(), DetectionStrategy::Gpt5Only)
                                                 on:change=move |_| strategy.set(DetectionStrategy::Gpt5Only)
                                             />
                                             <div class="option-content">
                                                 <strong>"AI Vision Analysis"</strong>
                                                 <p>"Pure AI-based blueprint interpretation and room detection"</p>
                                                 <small>"🧠 AI-powered • ⏱️ Slower (~1-2min)"</small>
                                             </div>
                                         </label>
                                     </div>
                                 }.into_any(),
                                 InputType::Svg => view! {
                                     <div class="strategy-group">
                                         <h4>"SVG-Based Detection"</h4>
                                         <label class="strategy-option">
                                             <input
                                                 type="radio"
                                                 name="strategy"
                                                 value="SvgOnly"
                                                 checked=move || matches!(strategy.get(), DetectionStrategy::SvgOnly)
                                                 on:change=move |_| strategy.set(DetectionStrategy::SvgOnly)
                                             />
                                             <div class="option-content">
                                                 <strong>"Direct SVG Parsing"</strong>
                                                 <p>"Algorithmic parsing of SVG elements - no AI required"</p>
                                                 <small>"⚡ Fastest (~1ms) • 📐 Geometric accuracy"</small>
                                             </div>
                                         </label>

                                         <label class="strategy-option">
                                             <input
                                                 type="radio"
                                                 name="strategy"
                                                 value="SvgWithVision"
                                                 checked=move || matches!(strategy.get(), DetectionStrategy::SvgWithVision)
                                                 on:change=move |_| strategy.set(DetectionStrategy::SvgWithVision)
                                             />
                                             <div class="option-content">
                                                 <strong>"SVG + AI Classification"</strong>
                                                 <p>"SVG parsing enhanced with AI room type identification"</p>
                                                 <small>"🎯 High accuracy • ⏱️ Moderate (~30s)"</small>
                                             </div>
                                         </label>

                                         <label class="strategy-option">
                                             <input
                                                 type="radio"
                                                 name="strategy"
                                                 value="SvgWithAiParser"
                                                 checked=move || matches!(strategy.get(), DetectionStrategy::SvgWithAiParser)
                                                 on:change=move |_| strategy.set(DetectionStrategy::SvgWithAiParser)
                                             />
                                             <div class="option-content">
                                                 <strong>"AI SVG Interpreter"</strong>
                                                 <p>"Use AI models to interpret SVG instead of geometric parsing"</p>
                                                 <small>"🧠 AI understanding • ⏱️ Variable time"</small>
                                             </div>
                                         </label>
                                     </div>
                                 }.into_any(),
                             }
                         }}
                    </div>
                </div>

                // File Upload Section
                <div class="file-upload">
                    {move || {
                        let current_input = input_type.get();
                        let accept_types = match current_input {
                            InputType::Json => ".json",
                            InputType::Image => ".png,.jpg,.jpeg",
                            InputType::Svg => ".svg",
                        };
                        let label_text = match current_input {
                            InputType::Json => "Upload JSON Lines File:",
                            InputType::Image => "Upload Blueprint Image (PNG/JPG):",
                            InputType::Svg => "Upload SVG Floorplan:",
                        };

                        view! {
                            <label for="file-input">{label_text}</label>
                            <input
                                type="file"
                                id="file-input"
                                accept=accept_types
                                node_ref=file_input_ref
                                on:change=on_file_change
                            />
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
                             InputType::Json => lines.get().is_empty(),
                             InputType::Image => rooms.get().is_empty(), // Images are processed on upload
                             InputType::Svg => svg_content.get().is_none(),
                         }
                     }
                 >
                     {move || {
                         if loading.get() {
                             "Processing..."
                         } else {
                             match input_type.get() {
                                 InputType::Json => "Detect Rooms",
                                 InputType::Image => "Rooms Detected",
                                 InputType::Svg => "Detect Rooms from SVG",
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
                     InputType::Json => view! {
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
                     InputType::Image => view! {
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
                     InputType::Svg => view! {
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
