use leptos::*;
use leptos_meta::*;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, HtmlInputElement};

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
}

#[derive(Debug, Serialize)]
struct DetectRequest {
    lines: Vec<Line>,
    area_threshold: f64,
}

#[derive(Debug, Deserialize)]
struct DetectResponse {
    rooms: Vec<Room>,
    total_rooms: usize,
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
    let (lines, set_lines) = create_signal(Vec::<Line>::new());
    let (rooms, set_rooms) = create_signal(Vec::<Room>::new());
    let (loading, set_loading) = create_signal(false);
    let (error, set_error) = create_signal(Option::<String>::None);
    let (area_threshold, set_area_threshold) = create_signal(100.0);

    let file_input_ref = create_node_ref::<html::Input>();
    let canvas_ref = create_node_ref::<html::Canvas>();

    // Handle file upload
    let on_file_change = move |_| {
        if let Some(input) = file_input_ref.get() {
            if let Some(files) = input.files() {
                if let Some(file) = files.get(0) {
                    let reader = web_sys::FileReader::new().unwrap();
                    let reader_clone = reader.clone();

                    let onload = Closure::wrap(Box::new(move |_event: web_sys::Event| {
                        if let Ok(result) = reader_clone.result() {
                            if let Some(text) = result.as_string() {
                                match serde_json::from_str::<Vec<Line>>(&text) {
                                    Ok(parsed_lines) => {
                                        set_lines.set(parsed_lines);
                                        set_error.set(None);
                                    }
                                    Err(e) => {
                                        set_error.set(Some(format!("Failed to parse JSON: {}", e)));
                                    }
                                }
                            }
                        }
                    }) as Box<dyn FnMut(_)>);

                    reader.set_onload(Some(onload.as_ref().unchecked_ref()));
                    onload.forget();

                    let _ = reader.read_as_text(&file);
                }
            }
        }
    };

    // Handle detection
    let on_detect = move |_| {
        let current_lines = lines.get();
        if current_lines.is_empty() {
            set_error.set(Some("Please upload a JSON file first".to_string()));
            return;
        }

        set_loading.set(true);
        set_error.set(None);

        spawn_local(async move {
            let request = DetectRequest {
                lines: current_lines,
                area_threshold: area_threshold.get(),
            };

            match detect_rooms(request).await {
                Ok(response) => {
                    set_rooms.set(response.rooms);
                    set_loading.set(false);
                }
                Err(e) => {
                    set_error.set(Some(format!("Detection failed: {}", e)));
                    set_loading.set(false);
                }
            }
        });
    };

    // Render canvas whenever lines or rooms change
    create_effect(move |_| {
        let current_lines = lines.get();
        let current_rooms = rooms.get();

        if let Some(canvas) = canvas_ref.get() {
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
                    <label for="file-input">"Upload Blueprint JSON:"</label>
                    <input
                        type="file"
                        id="file-input"
                        accept=".json"
                        node_ref=file_input_ref
                        on:change=on_file_change
                    />
                </div>

                <div class="threshold-control">
                    <label for="threshold">"Area Threshold:"</label>
                    <input
                        type="number"
                        id="threshold"
                        value=move || area_threshold.get()
                        on:input=move |ev| {
                            if let Ok(val) = event_target_value(&ev).parse::<f64>() {
                                set_area_threshold.set(val);
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
                    children=move |room: Room| {
                        view! {
                            <div class="room-card">
                                <h3>{format!("Room {}: {}", room.id, room.name_hint)}</h3>
                                <p>"Area: " {format!("{:.2}", room.area)}</p>
                                <p>"Bounding Box: [{:.1}, {:.1}, {:.1}, {:.1}]"
                                    room.bounding_box[0]
                                    room.bounding_box[1]
                                    room.bounding_box[2]
                                    room.bounding_box[3]
                                </p>
                            </div>
                        }
                    }
                />
            </div>
        </div>
    }
}

async fn detect_rooms(request: DetectRequest) -> Result<DetectResponse, String> {
    let client = reqwest::Client::new();

    let response = client
        .post("http://localhost:3000/detect")
        .json(&request)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Server error: {}", response.status()));
    }

    response
        .json::<DetectResponse>()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))
}
