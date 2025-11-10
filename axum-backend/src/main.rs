use axum::{
    extract::{DefaultBodyLimit, Json},
    http::{header, Method, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use base64::Engine;
use nalgebra::Point2;
use ordered_float::OrderedFloat;
use serde::{Deserialize, Serialize};
use tower_http::cors::{Any, CorsLayer};
use tracing::{info, warn};
use std::process::{Command, Stdio};
use std::io::Write;
use std::time::Instant;

mod graph_builder;
mod room_detector;
mod detector_orchestrator;
mod image_vectorizer;
mod image_preprocessor;
mod wall_merger;
mod connected_components;
mod vector_graph;
mod new_algorithms;

use graph_builder::*;
use room_detector::{detect_rooms, detect_rooms_simple};
use new_algorithms::detect_rust_floodfill_handler;
use vector_graph::detect_vector_graph_handler;

// Security limits to prevent DoS attacks
const MAX_LINES: usize = 10_000;
const MAX_COORDINATE_VALUE: f64 = 1_000_000.0;
const MIN_COORDINATE_VALUE: f64 = -1_000_000.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    /// Validate that point coordinates are within reasonable bounds
    fn is_valid(&self) -> bool {
        self.x.is_finite()
            && self.y.is_finite()
            && self.x >= MIN_COORDINATE_VALUE
            && self.x <= MAX_COORDINATE_VALUE
            && self.y >= MIN_COORDINATE_VALUE
            && self.y <= MAX_COORDINATE_VALUE
    }
}

impl Point {
    fn to_nalgebra(&self) -> Point2<f64> {
        Point2::new(self.x, self.y)
    }

    fn distance_to(&self, other: &Point) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }
}

impl PartialEq for Point {
    fn eq(&self, other: &Self) -> bool {
        // Use epsilon comparison for floating point equality
        const EPSILON: f64 = 1e-6;
        (self.x - other.x).abs() < EPSILON && (self.y - other.y).abs() < EPSILON
    }
}

impl Eq for Point {}

impl std::hash::Hash for Point {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Round to 6 decimal places for consistent hashing
        // This matches the epsilon used in PartialEq
        let x_rounded = (self.x * 1_000_000.0).round() as i64;
        let y_rounded = (self.y * 1_000_000.0).round() as i64;

        x_rounded.hash(state);
        y_rounded.hash(state);
    }
}

// Alternative: Use OrderedFloat wrapper for HashMap keys
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PointKey {
    x: OrderedFloat<f64>,
    y: OrderedFloat<f64>,
}

impl From<&Point> for PointKey {
    fn from(point: &Point) -> Self {
        // Round to avoid floating point precision issues
        const PRECISION: f64 = 1_000_000.0;
        PointKey {
            x: OrderedFloat((point.x * PRECISION).round() / PRECISION),
            y: OrderedFloat((point.y * PRECISION).round() / PRECISION),
        }
    }
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
    pub bounding_box: [f64; 4], // [min_x, min_y, max_x, max_y]
    pub area: f64,
    pub name_hint: String,
    pub points: Vec<Point>,
}

#[derive(Debug, Deserialize)]
struct DetectRoomsRequest {
    lines: Vec<Line>,
    #[serde(default = "default_area_threshold")]
    area_threshold: f64,
    #[serde(default = "default_door_threshold")]
    door_threshold: f64,
    #[serde(default = "default_coverage_threshold")]
    coverage_threshold: f64,
    #[serde(default = "default_outer_boundary_ratio")]
    outer_boundary_ratio: f64,
}

fn default_area_threshold() -> f64 {
    100.0
}

fn default_door_threshold() -> f64 {
    50.0  // Default door gap: 50 units
}

fn default_coverage_threshold() -> f64 {
    0.3  // 30% minimum height coverage for vertical dividers
}

fn default_outer_boundary_ratio() -> f64 {
    1.5  // Outer boundary must be 1.5x larger than second-largest room
}

#[derive(Debug, Serialize)]
struct DetectRoomsResponse {
    rooms: Vec<Room>,
    total_rooms: usize,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
    message: String,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: String,
    version: String,
}

async fn health_check() -> impl IntoResponse {
    Json(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

async fn detect_rooms_simple_handler(
    Json(request): Json<DetectRoomsRequest>,
) -> Result<Json<DetectRoomsResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Received simple detection request with {} lines", request.lines.len());

    // Validate input size
    if request.lines.len() > MAX_LINES {
        warn!(
            "Request rejected: too many lines ({} > {})",
            request.lines.len(),
            MAX_LINES
        );
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "INPUT_TOO_LARGE".to_string(),
                message: format!(
                    "Too many lines. Maximum allowed: {}. Received: {}",
                    MAX_LINES,
                    request.lines.len()
                ),
            }),
        ));
    }

    if request.lines.is_empty() {
        warn!("Empty lines input");
        return Ok(Json(DetectRoomsResponse {
            rooms: vec![],
            total_rooms: 0,
        }));
    }

    // Validate points
    for (idx, line) in request.lines.iter().enumerate() {
        if !line.start.is_valid() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "INVALID_POINT".to_string(),
                    message: format!("Invalid start point in line {}", idx),
                }),
            ));
        }
        if !line.end.is_valid() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "INVALID_POINT".to_string(),
                    message: format!("Invalid end point in line {}", idx),
                }),
            ));
        }
    }

    // Use simplified divider-based detection
    let rooms = detect_rooms_simple(&request.lines, request.area_threshold, request.coverage_threshold);
    info!("Detected {} rooms using simple algorithm", rooms.len());

    Ok(Json(DetectRoomsResponse {
        total_rooms: rooms.len(),
        rooms,
    }))
}

async fn detect_rooms_handler(
    Json(request): Json<DetectRoomsRequest>,
) -> Result<Json<DetectRoomsResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Received detection request with {} lines", request.lines.len());

    // Validate input size to prevent DoS
    if request.lines.len() > MAX_LINES {
        warn!(
            "Request rejected: too many lines ({} > {})",
            request.lines.len(),
            MAX_LINES
        );
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "INPUT_TOO_LARGE".to_string(),
                message: format!(
                    "Too many lines. Maximum allowed: {}. Received: {}",
                    MAX_LINES,
                    request.lines.len()
                ),
            }),
        ));
    }

    if request.lines.is_empty() {
        warn!("Empty lines input");
        return Ok(Json(DetectRoomsResponse {
            rooms: vec![],
            total_rooms: 0,
        }));
    }

    // Validate area threshold
    if !request.area_threshold.is_finite() || request.area_threshold < 0.0 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "INVALID_THRESHOLD".to_string(),
                message: "Area threshold must be a positive finite number".to_string(),
            }),
        ));
    }

    // Validate all points
    for (idx, line) in request.lines.iter().enumerate() {
        if !line.start.is_valid() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "INVALID_POINT".to_string(),
                    message: format!("Invalid start point in line {}", idx),
                }),
            ));
        }
        if !line.end.is_valid() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "INVALID_POINT".to_string(),
                    message: format!("Invalid end point in line {}", idx),
                }),
            ));
        }
    }

    // For JSON input, always use GraphOnly (cycle detection) - the algorithm that works
    let graph = if request.door_threshold > 0.0 {
        info!("Building graph with door threshold: {}", request.door_threshold);
        graph_builder::build_graph_with_door_threshold(&request.lines, request.door_threshold)
    } else {
        graph_builder::build_graph(&request.lines)
    };

    info!("Built graph with {} nodes and {} edges", graph.node_count(), graph.edge_count());

    // Detect rooms using cycle detection (the working algorithm from room-detection-rust)
    let rooms = room_detector::detect_rooms(&graph, request.area_threshold, 1.5);
    info!("Detected {} rooms using GraphOnly cycle detection", rooms.len());

    Ok(Json(DetectRoomsResponse {
        total_rooms: rooms.len(),
        rooms,
    }))
}

async fn detect_rooms_handler_old(
    Json(request): Json<DetectRoomsRequest>,
) -> Result<Json<DetectRoomsResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Received detection request with {} lines", request.lines.len());

    // Validate input size to prevent DoS
    if request.lines.len() > MAX_LINES {
        warn!(
            "Request rejected: too many lines ({} > {})",
            request.lines.len(),
            MAX_LINES
        );
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "INPUT_TOO_LARGE".to_string(),
                message: format!(
                    "Too many lines. Maximum allowed: {}. Received: {}",
                    MAX_LINES,
                    request.lines.len()
                ),
            }),
        ));
    }

    if request.lines.is_empty() {
        warn!("Empty lines input");
        return Ok(Json(DetectRoomsResponse {
            rooms: vec![],
            total_rooms: 0,
        }));
    }

    // Validate area threshold
    if !request.area_threshold.is_finite() || request.area_threshold < 0.0 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "INVALID_THRESHOLD".to_string(),
                message: "Area threshold must be a positive finite number".to_string(),
            }),
        ));
    }

    // Validate all points
    for (idx, line) in request.lines.iter().enumerate() {
        if !line.start.is_valid() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "INVALID_POINT".to_string(),
                    message: format!("Invalid start point in line {}: coordinates must be finite and within [{}, {}]", idx, MIN_COORDINATE_VALUE, MAX_COORDINATE_VALUE),
                }),
            ));
        }
        if !line.end.is_valid() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "INVALID_POINT".to_string(),
                    message: format!("Invalid end point in line {}: coordinates must be finite and within [{}, {}]", idx, MIN_COORDINATE_VALUE, MAX_COORDINATE_VALUE),
                }),
            ));
        }
    }

    // Build graph from lines with door gap detection
    let graph = if request.door_threshold > 0.0 {
        info!("Building graph with door threshold: {}", request.door_threshold);
        graph_builder::build_graph_with_door_threshold(&request.lines, request.door_threshold)
    } else {
        build_graph(&request.lines)
    };
    info!("Built graph with {} nodes and {} edges", graph.node_count(), graph.edge_count());

    // Detect rooms (cycles)
    let rooms = detect_rooms(&graph, request.area_threshold, request.outer_boundary_ratio);
    info!("Detected {} rooms", rooms.len());

    Ok(Json(DetectRoomsResponse {
        total_rooms: rooms.len(),
        rooms,
    }))
}

/// Enhanced detection request with orchestrator support
#[derive(Debug, Deserialize)]
struct EnhancedDetectRequest {
    lines: Vec<Line>,
    #[serde(default)]
    image_base64: Option<String>,
    #[serde(default = "default_area_threshold")]
    area_threshold: f64,
    #[serde(default = "default_door_threshold")]
    door_threshold: f64,
    #[serde(default)]
    strategy: Option<detector_orchestrator::CombinationStrategy>,
    #[serde(default)]
    enable_vision: Option<bool>,
    #[serde(default)]
    enable_yolo: Option<bool>,
}

/// SVG detection request
#[derive(Debug, Deserialize)]
struct SvgDetectRequest {
    svg_content: String,
    #[serde(default = "default_area_threshold")]
    area_threshold: f64,
    #[serde(default = "default_door_threshold")]
    door_threshold: f64,
    #[serde(default)]
    strategy: Option<detector_orchestrator::CombinationStrategy>,
    #[serde(default)]
    enable_vision: Option<bool>,
}

/// Enhanced detection handler using the orchestrator
async fn enhanced_detect_handler(
    Json(request): Json<EnhancedDetectRequest>,
) -> Result<Json<detector_orchestrator::DetectionResult>, (StatusCode, Json<ErrorResponse>)> {
    info!("Received enhanced detection request with {} lines", request.lines.len());

    // Validate input (same as regular detect)
    if request.lines.len() > MAX_LINES {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "INPUT_TOO_LARGE".to_string(),
                message: format!("Too many lines. Max: {}", MAX_LINES),
            }),
        ));
    }

    // Validate points
    for (idx, line) in request.lines.iter().enumerate() {
        if !line.start.is_valid() || !line.end.is_valid() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "INVALID_POINT".to_string(),
                    message: format!("Invalid point in line {}", idx),
                }),
            ));
        }
    }

    // Decode image if provided
    let image_bytes = if let Some(ref b64) = request.image_base64 {
        match base64::engine::general_purpose::STANDARD.decode(b64) {
            Ok(bytes) => Some(bytes),
            Err(e) => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: "INVALID_IMAGE".to_string(),
                        message: format!("Failed to decode base64 image: {}", e),
                    }),
                ));
            }
        }
    } else {
        None
    };

    // Build orchestrator config
    let mut config = detector_orchestrator::DetectorConfig {
        area_threshold: request.area_threshold,
        door_threshold: request.door_threshold,
        enable_vision: request.enable_vision.unwrap_or(false),
        enable_yolo: request.enable_yolo.unwrap_or(false),
        strategy: request
            .strategy
            .unwrap_or(detector_orchestrator::CombinationStrategy::GraphOnly),
        confidence_threshold: 0.75, // Default for enhanced endpoint
        vision_model: std::env::var("VISION_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string()),
    };

    // Auto-enable vision if API key is set and strategy requires it
    if matches!(
        config.strategy,
        detector_orchestrator::CombinationStrategy::GraphWithVision
            | detector_orchestrator::CombinationStrategy::BestAvailable
            | detector_orchestrator::CombinationStrategy::Ensemble
    ) && std::env::var("OPENAI_API_KEY").is_ok()
    {
        config.enable_vision = true;
    }

    // Create orchestrator and run detection
    let orchestrator = detector_orchestrator::DetectorOrchestrator::new(config);

    match orchestrator
        .detect_rooms(&request.lines, image_bytes.as_deref(), None)
        .await
    {
        Ok(result) => {
            info!(
                "Enhanced detection completed: {} rooms, method: {}, time: {}ms",
                result.rooms.len(),
                result.method_used,
                result.execution_time_ms
            );
            Ok(Json(result))
        }
        Err(e) => {
            warn!("Enhanced detection failed: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "DETECTION_FAILED".to_string(),
                    message: format!("Detection failed: {}", e),
                }),
            ))
        }
    }
}

/// SVG detection handler
async fn svg_detect_handler(
    Json(request): Json<SvgDetectRequest>,
) -> Result<Json<detector_orchestrator::DetectionResult>, (StatusCode, Json<ErrorResponse>)> {
    info!("Received SVG detection request with {} chars of SVG content", request.svg_content.len());

    // Validate input size
    if request.svg_content.len() > 10 * 1024 * 1024 { // 10MB limit
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "INPUT_TOO_LARGE".to_string(),
                message: "SVG content too large. Maximum 10MB allowed.".to_string(),
            }),
        ));
    }

    if request.svg_content.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "EMPTY_SVG".to_string(),
                message: "SVG content cannot be empty".to_string(),
            }),
        ));
    }

    // Build orchestrator config
    let mut config = detector_orchestrator::DetectorConfig {
        area_threshold: request.area_threshold,
        door_threshold: request.door_threshold,
        enable_vision: request.enable_vision.unwrap_or(false),
        enable_yolo: false, // SVG doesn't support YOLO yet
        strategy: request
            .strategy
            .unwrap_or(detector_orchestrator::CombinationStrategy::SvgOnly),
        confidence_threshold: 0.75, // Default for SVG endpoint
        vision_model: std::env::var("VISION_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string()),
    };

    // Auto-enable vision if API key is set and strategy requires it
    if matches!(
        config.strategy,
        detector_orchestrator::CombinationStrategy::SvgWithVision
    ) && std::env::var("OPENAI_API_KEY").is_ok()
    {
        config.enable_vision = true;
    }

    // Create orchestrator and run detection
    let orchestrator = detector_orchestrator::DetectorOrchestrator::new(config);

    match orchestrator
        .detect_rooms(&[], None, Some(&request.svg_content))
        .await
    {
        Ok(result) => {
            info!(
                "SVG detection completed: {} rooms, method: {}, time: {}ms",
                result.rooms.len(),
                result.method_used,
                result.execution_time_ms
            );
            Ok(Json(result))
        }
        Err(e) => {
            warn!("SVG detection failed: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "DETECTION_FAILED".to_string(),
                    message: format!("SVG detection failed: {}", e),
                }),
            ))
        }
    }
}

#[derive(Deserialize)]
struct UploadImageRequest {
    /// Base64 encoded image (PNG or JPEG)
    image: String,
    /// Area threshold for room detection
    #[serde(default = "default_area_threshold")]
    area_threshold: f64,
    /// Door gap bridging threshold
    #[serde(default)]
    door_threshold: Option<f64>,
}

async fn upload_image_handler(
    Json(payload): Json<UploadImageRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    info!("Received image upload request");

    // Decode base64 image
    let image_bytes = base64::engine::general_purpose::STANDARD
        .decode(&payload.image)
        .map_err(|e| {
            warn!("Failed to decode base64 image: {}", e);
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "INVALID_IMAGE".to_string(),
                    message: format!("Failed to decode base64 image: {}", e),
                }),
            )
        })?;

    info!("Image decoded, size: {} bytes", image_bytes.len());

    // Vectorize image to extract lines
    let extracted_lines = tokio::spawn(async move {
        image_vectorizer::vectorize_image_ai(&image_bytes).await
    }).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "VECTORIZATION_FAILED".to_string(),
                message: format!("Async vectorization failed: {}", e),
            }),
        )
    })?.map_err(|e| {
        warn!("Vectorization failed: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "VECTORIZATION_FAILED".to_string(),
                message: format!("Failed to vectorize image: {}", e),
            }),
        )
    })?;

    info!("Vectorization complete, extracted {} lines", extracted_lines.len());

    // Convert to Line format expected by detection algorithm
    let lines: Vec<Line> = extracted_lines
        .into_iter()
        .map(|l| Line {
            start: crate::Point { x: l.start.x, y: l.start.y },
            end: crate::Point { x: l.end.x, y: l.end.y },
            is_load_bearing: l.is_load_bearing,
        })
        .collect();

    // Build graph with door detection
    let door_threshold = payload.door_threshold.unwrap_or(50.0);
    let graph = build_graph_with_door_threshold(&lines, door_threshold);

    // Detect rooms
    let rooms = detect_rooms(&graph, payload.area_threshold, 1.5); // Default outer boundary ratio
    info!("Detected {} rooms", rooms.len());

    Ok(Json(serde_json::json!({
        "total_rooms": rooms.len(),
        "rooms": rooms,
        "lines_extracted": lines.len(),
        "vectorization_complete": true,
    })))
}

#[derive(Debug, Deserialize)]
struct VectorizeBlueprintRequest {
    /// Base64-encoded blueprint image
    image: String,
    /// Strategy for vectorization: hybrid_vision, vtracer_only, or gpt5_only
    #[serde(default = "default_vectorization_strategy")]
    strategy: String,
    /// Confidence threshold for GPT-5 vision (0.0-1.0)
    #[serde(default = "default_confidence_threshold")]
    confidence_threshold: f64,
    /// Area threshold for room detection
    #[serde(default = "default_area_threshold")]
    area_threshold: f64,
    /// Door gap threshold
    #[serde(default = "default_door_threshold")]
    door_threshold: f64,
    /// Vision model to use (gpt-4o-mini, gpt-4o, gpt-5)
    #[serde(default = "default_vision_model_api")]
    vision_model: String,
}

fn default_vision_model_api() -> String {
    std::env::var("VISION_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string())
}

fn default_vectorization_strategy() -> String {
    "hybrid_vision".to_string()
}

fn default_confidence_threshold() -> f64 {
    0.75
}

#[derive(Debug, Serialize)]
struct VectorizeBlueprintResponse {
    walls: Vec<WallWithSource>,
    rooms: Vec<EnhancedRoomResponse>,
    metadata: VectorizationMetadata,
}

#[derive(Debug, Serialize)]
struct WallWithSource {
    start: Point,
    end: Point,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<String>, // "vtracer", "gpt5", or "consensus"
}

#[derive(Debug, Serialize)]
struct EnhancedRoomResponse {
    id: usize,
    bounding_box: [f64; 4],
    area: f64,
    name_hint: String,
    points: Vec<Point>,
    #[serde(skip_serializing_if = "Option::is_none")]
    room_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    confidence: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    center: Option<Point>,
}

#[derive(Debug, Serialize)]
struct VectorizationMetadata {
    vtracer_walls_count: usize,
    gpt5_walls_count: usize,
    merged_walls_count: usize,
    gpt5_confidence: f64,
    method_used: String,
    execution_time_ms: u128,
}

async fn vectorize_blueprint_handler(
    Json(payload): Json<VectorizeBlueprintRequest>,
) -> Result<Json<VectorizeBlueprintResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Received vectorize-blueprint request (strategy: {})", payload.strategy);

    // Decode base64 image
    let image_bytes = base64::engine::general_purpose::STANDARD
        .decode(&payload.image)
        .map_err(|e| {
            warn!("Failed to decode base64 image: {}", e);
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "INVALID_IMAGE".to_string(),
                    message: format!("Failed to decode base64 image: {}", e),
                }),
            )
        })?;

    info!("Image decoded, size: {} bytes", image_bytes.len());

    // Create orchestrator with hybrid vision strategy
    let config = detector_orchestrator::DetectorConfig {
        area_threshold: payload.area_threshold,
        door_threshold: payload.door_threshold,
        enable_vision: true,
        enable_yolo: false,
        strategy: match payload.strategy.as_str() {
            "vtracer_only" => detector_orchestrator::CombinationStrategy::VTracerOnly,
            "gpt5_only" => detector_orchestrator::CombinationStrategy::GraphWithVision,
            _ => detector_orchestrator::CombinationStrategy::HybridVision,
        },
        confidence_threshold: payload.confidence_threshold,
        vision_model: payload.vision_model,
    };

    let orchestrator = detector_orchestrator::DetectorOrchestrator::new(config);

    // Run detection (for hybrid vision, lines are extracted internally)
    let result = orchestrator
        .detect_rooms(&[], Some(&image_bytes), None)
        .await
        .map_err(|e| {
            warn!("Detection failed: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "DETECTION_FAILED".to_string(),
                    message: format!("Failed to detect rooms: {}", e),
                }),
            )
        })?;

    info!(
        "Detection complete: {} rooms detected using {} in {}ms",
        result.rooms.len(),
        result.method_used,
        result.execution_time_ms
    );

    // Extract walls from metadata (if available from hybrid vision)
    let walls: Vec<WallWithSource> = result
        .metadata
        .merged_walls
        .as_ref()
        .map(|merged_walls| {
            merged_walls
                .iter()
                .map(|w| WallWithSource {
                    start: Point { x: w.start.x, y: w.start.y },
                    end: Point { x: w.end.x, y: w.end.y },
                    source: w.source.clone(),
                })
                .collect()
        })
        .unwrap_or_default();

    // Convert rooms to response format
    let rooms: Vec<EnhancedRoomResponse> = result
        .rooms
        .iter()
        .map(|r| EnhancedRoomResponse {
            id: r.room.id,
            bounding_box: r.room.bounding_box,
            area: r.room.area,
            name_hint: r.room.name_hint.clone(),
            points: r.room.points.clone(),
            room_type: r.room_type.clone(),
            confidence: r.confidence,
            center: r.room_type.as_ref().map(|_| {
                Point {
                    x: (r.room.bounding_box[0] + r.room.bounding_box[2]) / 2.0,
                    y: (r.room.bounding_box[1] + r.room.bounding_box[3]) / 2.0,
                }
            }),
        })
        .collect();

    // Build metadata from detection result
    let metadata = VectorizationMetadata {
        vtracer_walls_count: result.metadata.vtracer_walls_count.unwrap_or(0),
        gpt5_walls_count: result.metadata.gpt5_walls_count.unwrap_or(0),
        merged_walls_count: result.metadata.merged_walls_count.unwrap_or(0),
        gpt5_confidence: result.metadata.gpt5_confidence.unwrap_or(0.0),
        method_used: result.method_used,
        execution_time_ms: result.execution_time_ms,
    };

    Ok(Json(VectorizeBlueprintResponse {
        walls,
        rooms,
        metadata,
    }))
}

#[derive(Debug, Deserialize)]
struct ImageDetectRequest {
    image: String,  // base64 encoded image
    #[serde(default = "default_threshold")]
    threshold: u8,
    #[serde(default = "default_min_area")]
    min_area: usize,
    #[serde(default = "default_max_area_ratio")]
    max_area_ratio: f32,
}

fn default_threshold() -> u8 {
    200
}

fn default_min_area() -> usize {
    200  // Much smaller for blueprint detection
}

fn default_max_area_ratio() -> f32 {
    0.3
}

/// Detect rooms using connected components on the image
async fn detect_rooms_connected_components_handler(
    Json(request): Json<ImageDetectRequest>,
) -> Result<Json<DetectRoomsResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Received original connected components detection request");

    // Decode base64 image
    let engine = base64::engine::general_purpose::STANDARD;
    let img_bytes = engine
        .decode(&request.image)
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "INVALID_BASE64".to_string(),
                    message: format!("Failed to decode base64 image: {}", e),
                }),
            )
        })?;

    info!("Image decoded, size: {} bytes", img_bytes.len());

    // Load image
    let img = image::load_from_memory(&img_bytes)
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "INVALID_IMAGE".to_string(),
                    message: format!("Failed to load image: {}", e),
                }),
            )
        })?
        .to_luma8();

    info!("Image loaded: {}x{}", img.width(), img.height());

    // Detect rooms using original connected components
    let rooms = connected_components::detect_rooms_connected_components(
        &img,
        request.threshold,
        request.min_area,
        request.max_area_ratio,
    );

    info!("Detected {} rooms using original connected components", rooms.len());

    Ok(Json(DetectRoomsResponse {
        total_rooms: rooms.len(),
        rooms,
    }))
}

async fn detect_rooms_connected_components_enhanced_handler(
    Json(request): Json<ImageDetectRequest>,
) -> Result<Json<DetectRoomsResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Received enhanced connected components detection request");

    // Decode base64 image
    let engine = base64::engine::general_purpose::STANDARD;
    let img_bytes = engine
        .decode(&request.image)
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "INVALID_BASE64".to_string(),
                    message: format!("Failed to decode base64 image: {}", e),
                }),
            )
        })?;

    info!("Image decoded, size: {} bytes", img_bytes.len());

    // Load image
    let img = image::load_from_memory(&img_bytes)
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "INVALID_IMAGE".to_string(),
                    message: format!("Failed to load image: {}", e),
                }),
            )
        })?
        .to_luma8();

    info!("Image loaded: {}x{}", img.width(), img.height());

    let start_time = Instant::now();

    // Enhanced flood fill with morphological operations
    let binary = connected_components::threshold_image_enhanced(&img, request.threshold);
    let components = connected_components::find_connected_components_enhanced(&binary, request.min_area, request.max_area_ratio);
    
    let mut rooms = Vec::new();
    let mut room_id = 0;
    for (area, bbox) in components.iter() {
        let (min_x, min_y, max_x, max_y) = *bbox;

        // Create bounding box in normalized coordinates (0-1000 scale for compatibility)
        let norm_x = (min_x as f64 / img.width() as f64) * 1000.0;
        let norm_y = (min_y as f64 / img.height() as f64) * 1000.0;
        let norm_max_x = (max_x as f64 / img.width() as f64) * 1000.0;
        let norm_max_y = (max_y as f64 / img.height() as f64) * 1000.0;

        let bounding_box = [norm_x, norm_y, norm_max_x, norm_max_y];

        // Create corner points for the room polygon
        let points = vec![
            crate::Point { x: norm_x, y: norm_y },
            crate::Point { x: norm_max_x, y: norm_y },
            crate::Point { x: norm_max_x, y: norm_max_y },
            crate::Point { x: norm_x, y: norm_max_y },
        ];

        rooms.push(Room {
            id: room_id,
            bounding_box,
            area: *area as f64,
            name_hint: connected_components::generate_room_name(*area as f64),
            points,
        });

        room_id += 1;
    }
    
    let execution_time = start_time.elapsed().as_millis() as u64;
    info!("Detected {} rooms using enhanced connected components in {}ms", rooms.len(), execution_time);

    Ok(Json(DetectRoomsResponse {
        total_rooms: rooms.len(),
        rooms,
    }))
}

async fn detect_python_cc_handler(
    Json(request): Json<ImageDetectRequest>,
) -> Result<Json<DetectRoomsResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Received Python CC detection request");

    // Create JSON input for Python script
    let input_json = serde_json::json!({
        "image": request.image
    });

    // Call Python script via subprocess
    let python_path = ".venv/bin/python";
    let script_path = "room_detection_image_api.py";

    let mut child = Command::new(python_path)
        .arg(script_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "PYTHON_EXEC_ERROR".to_string(),
                    message: format!("Failed to spawn Python process: {}", e),
                }),
            )
        })?;

    // Write JSON to stdin
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(input_json.to_string().as_bytes())
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: "PYTHON_STDIN_ERROR".to_string(),
                        message: format!("Failed to write to Python stdin: {}", e),
                    }),
                )
            })?;
    }

    // Wait for Python script to complete
    let output = child.wait_with_output().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "PYTHON_WAIT_ERROR".to_string(),
                message: format!("Failed to wait for Python process: {}", e),
            }),
        )
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "PYTHON_SCRIPT_ERROR".to_string(),
                message: format!("Python script failed: {}", stderr),
            }),
        ));
    }

    // Parse Python output
    let stdout = String::from_utf8_lossy(&output.stdout);
    let python_response: serde_json::Value = serde_json::from_str(&stdout).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "PYTHON_PARSE_ERROR".to_string(),
                message: format!("Failed to parse Python output: {}", e),
            }),
        )
    })?;

    // Extract rooms from Python response
    let rooms: Vec<Room> = serde_json::from_value(python_response["rooms"].clone()).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "PYTHON_ROOMS_ERROR".to_string(),
                message: format!("Failed to extract rooms from Python response: {}", e),
            }),
        )
    })?;

    info!("Detected {} rooms using Python CC", rooms.len());

    Ok(Json(DetectRoomsResponse {
        total_rooms: rooms.len(),
        rooms,
    }))
}



/// Detect rooms using graph-based detection on rasterized image
async fn detect_rooms_graph_image_handler(
    Json(request): Json<ImageDetectRequest>,
) -> Result<Json<DetectRoomsResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Received graph-image detection request");

    // Decode base64 image
    let engine = base64::engine::general_purpose::STANDARD;
    let img_bytes = engine
        .decode(&request.image)
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "INVALID_BASE64".to_string(),
                    message: format!("Failed to decode base64 image: {}", e),
                }),
            )
        })?;

    info!("Image decoded, size: {} bytes", img_bytes.len());

    // Use VTracer to extract lines from the image
    let config = detector_orchestrator::DetectorConfig {
        area_threshold: 100.0,
        door_threshold: 50.0,
        enable_vision: false,
        enable_yolo: false,
        strategy: detector_orchestrator::CombinationStrategy::VTracerOnly,
        confidence_threshold: 0.75,
        vision_model: "gpt-4o-mini".to_string(),
    };

    let orchestrator = detector_orchestrator::DetectorOrchestrator::new(config);

    let empty_lines: Vec<Line> = Vec::new();
    let result = orchestrator
        .detect_rooms(&empty_lines, Some(&img_bytes), None)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "DETECTION_FAILED".to_string(),
                    message: format!("Graph detection failed: {}", e),
                }),
            )
        })?;

    let rooms: Vec<Room> = result
        .rooms
        .iter()
        .map(|r| r.room.clone())
        .collect();

    info!("Detected {} rooms using graph-image detection", rooms.len());

    Ok(Json(DetectRoomsResponse {
        total_rooms: rooms.len(),
        rooms,
    }))
}

/// Create the Axum app with all routes and middleware
/// This is exposed for integration testing
async fn test_handler() -> Json<Vec<serde_json::Value>> {
    Json(vec![
        serde_json::json!({
            "id": "room_001",
            "bounding_box": [50.0, 50.0, 200.0, 300.0],
            "name_hint": "Entry Hall"
        }),
        serde_json::json!({
            "id": "room_002",
            "bounding_box": [250.0, 50.0, 700.0, 500.0],
            "name_hint": "Main Office"
        }),
    ])
}

/// GPT-4o validation handler - proxies request to OpenAI API
async fn gpt4o_validation_handler(
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    info!("GPT-4o validation request received");

    // Get OpenAI API key from environment
    let api_key = std::env::var("OPENAI_API_KEY")
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "OPENAI_API_KEY not configured".to_string()))?;

    // Forward request to OpenAI API
    let client = reqwest::Client::new();
    let response = client
        .post("https://api.openai.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("OpenAI API request failed: {}", e)))?;

    let status = response.status();
    let body = response.text().await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to read response: {}", e)))?;

    if status.is_success() {
        info!("GPT-4o validation completed successfully");
        Ok((StatusCode::OK, body))
    } else {
        warn!("GPT-4o validation failed with status: {}", status);
        Err((StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR), body))
    }
}

pub fn create_app() -> Router {
    // Configure CORS from environment or use localhost for development
    let allowed_origins = std::env::var("ALLOWED_ORIGINS")
        .unwrap_or_else(|_| "http://localhost:8080,http://127.0.0.1:8080,http://localhost:8081,http://127.0.0.1:8081,http://localhost:8082,http://127.0.0.1:8082,http://localhost:9090,http://127.0.0.1:9090".to_string());

    let origins: Vec<_> = allowed_origins
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();

    let cors = if origins.is_empty() {
        // Fallback to Any only if no valid origins configured (not recommended for production)
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods([Method::GET, Method::POST])
            .allow_headers([header::CONTENT_TYPE])
    } else {
        CorsLayer::new()
            .allow_origin(origins)
            .allow_methods([Method::GET, Method::POST])
            .allow_headers([header::CONTENT_TYPE])
    };

    Router::new()
        .route("/health", get(health_check))
        .route("/detect", post(detect_rooms_handler))
        .route("/detect/simple", post(detect_rooms_simple_handler))
        .route("/detect/enhanced", post(enhanced_detect_handler))
        .route("/detect/svg", post(svg_detect_handler))
        .route("/detect/connected-components", post(detect_rooms_connected_components_handler))
        .route("/detect/rust-floodfill", post(detect_rust_floodfill_handler))
        .route("/detect/vector-graph", post(detect_vector_graph_handler))
        .route("/detect/graph-image", post(detect_rooms_graph_image_handler))
        .route("/detect/python-cc", post(detect_python_cc_handler))
        .route("/upload-image", post(upload_image_handler))
        .route("/vectorize-blueprint", post(vectorize_blueprint_handler))
        .route("/validate/gpt4o", post(gpt4o_validation_handler))
        .route("/test", get(test_handler))
        .layer(DefaultBodyLimit::max(10 * 1024 * 1024)) // 10MB max for images
        .layer(cors)
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    info!("Starting Floorplan Backend Server");

    let app = create_app();

    let addr = "0.0.0.0:3000";
    info!("Server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
