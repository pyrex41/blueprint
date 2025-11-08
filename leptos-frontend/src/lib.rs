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
    Simple,
    GraphOnly,
    GraphWithVision,
    YoloOnly,
    BestAvailable,
    Ensemble,
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
        }
    }

    fn description(&self) -> &'static str {
        match self {
            Self::Simple => "Divider-based for rectangular rooms (<1ms)",
            Self::GraphOnly => "Cycle detection for complex shapes (<1ms)",
            Self::GraphWithVision => "Geometric + AI classification (~54s)",
            Self::YoloOnly => "ML-based detection (not available yet)",
            Self::BestAvailable => "Auto-fallback to best available method",
            Self::Ensemble => "Run all methods, return best result",
        }
    }
}

#[derive(Debug, Serialize)]
struct DetectRequest {
    lines: Vec<Line>,
    area_threshold: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    door_threshold: Option<f64>,
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
        <Title text="Floorplan Room Detector"/>
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
    let strategy = RwSignal::new(DetectionStrategy::Simple);
    let method_used = RwSignal::new(Option::<String>::None);
    let execution_time = RwSignal::new(Option::<u64>::None);

    let file_input_ref = NodeRef::<leptos::html::Input>::new();
    let canvas_ref = NodeRef::<leptos::html::Canvas>::new();

    // Handle file upload (both JSON and images)
    let on_file_change = move |_| {
        if let Some(input) = file_input_ref.get() {
            if let Some(files) = input.files() {
                if let Some(file) = files.get(0) {
                    let file_type = file.type_();
                    let file_name = file.name();

                    // Check if it's a JSON file
                    if file_name.ends_with(".json") || file_type == "application/json" {
                        // Handle JSON file
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
                    }
                    // Check if it's an image file
                    else if file_type.starts_with("image/") {
                        // Handle image file - read as base64 and send to /upload-image endpoint
                        loading.set(true);
                        error.set(None);

                        let reader = web_sys::FileReader::new().unwrap();
                        let reader_clone = reader.clone();

                        let current_area_threshold = area_threshold.get();
                        let current_door_threshold = door_threshold.get();

                        let onload = Closure::wrap(Box::new(move |_event: web_sys::Event| {
                            if let Ok(result) = reader_clone.result() {
                                if let Some(data_url) = result.as_string() {
                                    // Extract base64 data (remove "data:image/...;base64," prefix)
                                    if let Some(base64_data) = data_url.split(',').nth(1) {
                                        let base64_owned = base64_data.to_string();

                                        spawn_local(async move {
                                            #[derive(Serialize)]
                                            struct UploadImageRequest {
                                                image: String,
                                                area_threshold: f64,
                                                door_threshold: Option<f64>,
                                            }

                                            let request = UploadImageRequest {
                                                image: base64_owned,
                                                area_threshold: current_area_threshold,
                                                door_threshold: Some(current_door_threshold),
                                            };

                                            match send_image_request(request).await {
                                                Ok(response) => {
                                                    // Extract rooms from response
                                                    if let Some(rooms_array) = response.get("rooms") {
                                                        if let Ok(parsed_rooms) = serde_json::from_value::<Vec<Room>>(rooms_array.clone()) {
                                                            rooms.set(parsed_rooms);
                                                        }
                                                    }
                                                    // Extract lines for visualization
                                                    if let Some(lines_count) = response.get("lines_extracted") {
                                                        // Note: We don't get the actual lines back from /upload-image
                                                        // So we just clear the lines display
                                                        lines.set(Vec::new());
                                                    }
                                                    loading.set(false);
                                                    error.set(None);
                                                }
                                                Err(e) => {
                                                    error.set(Some(format!("Image upload failed: {}", e)));
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
                        error.set(Some("Please upload a JSON or image file".to_string()));
                    }
                }
            }
        }
    };

    // Handle detection
    let on_detect = move |_| {
        let current_lines = lines.get();
        if current_lines.is_empty() {
            error.set(Some("Please upload a JSON file first".to_string()));
            return;
        }

        loading.set(true);
        error.set(None);
        method_used.set(None);
        execution_time.set(None);

        let current_strategy = strategy.get();
        let current_area_threshold = area_threshold.get();
        let current_door_threshold = door_threshold.get();

        spawn_local(async move {
            let request = DetectRequest {
                lines: current_lines,
                area_threshold: current_area_threshold,
                door_threshold: Some(current_door_threshold),
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
                <h1>"Floorplan Room Detector"</h1>
                <p>"Upload a JSON file with blueprint lines to detect rooms"</p>
            </header>

            <div class="controls">
                <div class="file-upload">
                    <label for="file-input">"Upload Blueprint (JSON or Image):"</label>
                    <input
                        type="file"
                        id="file-input"
                        accept=".json,.png,.jpg,.jpeg"
                        node_ref=file_input_ref
                        on:change=on_file_change
                    />
                </div>

                <div class="threshold-control">
                    <label for="strategy">"Detection Strategy:"</label>
                    <select
                        id="strategy"
                        on:change=move |ev| {
                            let val = ev.target().unwrap().unchecked_into::<web_sys::HtmlSelectElement>().value();
                            let new_strategy = match val.as_str() {
                                "Simple" => DetectionStrategy::Simple,
                                "GraphOnly" => DetectionStrategy::GraphOnly,
                                "GraphWithVision" => DetectionStrategy::GraphWithVision,
                                "YoloOnly" => DetectionStrategy::YoloOnly,
                                "BestAvailable" => DetectionStrategy::BestAvailable,
                                "Ensemble" => DetectionStrategy::Ensemble,
                                _ => DetectionStrategy::Simple,
                            };
                            strategy.set(new_strategy);
                        }
                    >
                        <option value="Simple" selected>"Simple (Divider-based)"</option>
                        <option value="GraphOnly">"Graph (Cycle Detection)"</option>
                        <option value="GraphWithVision">"Graph + Vision (AI)"</option>
                        <option value="YoloOnly">"YOLO Only (Not Ready)"</option>
                        <option value="BestAvailable">"Best Available"</option>
                        <option value="Ensemble">"Ensemble (All Methods)"</option>
                    </select>
                    <p style="font-size: 12px; color: #666; margin-top: 4px;">
                        {move || strategy.get().description()}
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

                <button
                    class="detect-button"
                    on:click=on_detect
                    disabled=move || loading.get() || lines.get().is_empty()
                >
                    {move || if loading.get() { "Detecting..." } else { "Detect Rooms" }}
                </button>
            </div>

            {move || error.get().map(|err| view! {
                <div class="error">
                    {err}
                </div>
            })}


            <div class="stats">
                <p>"Lines: " {move || lines.get().len()}</p>
                <p>"Rooms Detected: " {move || rooms.get().len()}</p>
                {move || method_used.get().map(|method| view! {
                    <p>"Method: " {method}</p>
                })}
                {move || execution_time.get().map(|time| view! {
                    <p>"Time: " {format!("{}ms", time)}</p>
                })}
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

async fn send_image_request<T: Serialize>(request: T) -> Result<serde_json::Value, String> {
    let client = reqwest::Client::new();

    let response = client
        .post("http://localhost:3000/upload-image")
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
    let endpoint = if strategy == DetectionStrategy::Simple {
        "http://localhost:3000/detect/simple"
    } else {
        "http://localhost:3000/detect/enhanced"
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

#[wasm_bindgen(start)]
pub fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App);
}
