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

mod graph_builder;
mod room_detector;
mod detector_orchestrator;
mod image_vectorizer;
mod image_preprocessor;
mod wall_merger;

use graph_builder::*;
use room_detector::{detect_rooms, detect_rooms_simple};

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
        .detect_rooms(&request.lines, image_bytes.as_deref())
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
    let extracted_lines = image_vectorizer::vectorize_image(&image_bytes).map_err(|e| {
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
            "vtracer_only" => detector_orchestrator::CombinationStrategy::GraphOnly,
            "gpt5_only" => detector_orchestrator::CombinationStrategy::GraphWithVision,
            _ => detector_orchestrator::CombinationStrategy::HybridVision,
        },
    };

    let orchestrator = detector_orchestrator::DetectorOrchestrator::new(config);

    // Run detection (for hybrid vision, lines are extracted internally)
    let result = orchestrator
        .detect_rooms(&[], Some(&image_bytes))
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

    // For hybrid vision, we need to extract wall information
    // For now, return empty walls array (could be enhanced to return merged walls)
    let walls = vec![]; // TODO: Extract from merge result

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

    // Build metadata
    let metadata = VectorizationMetadata {
        vtracer_walls_count: 0, // TODO: Get from merge result
        gpt5_walls_count: 0,    // TODO: Get from merge result
        merged_walls_count: 0,  // TODO: Get from merge result
        gpt5_confidence: 0.0,   // TODO: Get from vision result
        method_used: result.method_used,
        execution_time_ms: result.execution_time_ms,
    };

    Ok(Json(VectorizeBlueprintResponse {
        walls,
        rooms,
        metadata,
    }))
}

/// Create the Axum app with all routes and middleware
/// This is exposed for integration testing
pub fn create_app() -> Router {
    // Configure CORS from environment or use localhost for development
    let allowed_origins = std::env::var("ALLOWED_ORIGINS")
        .unwrap_or_else(|_| "http://localhost:8080,http://127.0.0.1:8080,http://localhost:8081,http://127.0.0.1:8081".to_string());

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
        .route("/upload-image", post(upload_image_handler))
        .route("/vectorize-blueprint", post(vectorize_blueprint_handler))
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
