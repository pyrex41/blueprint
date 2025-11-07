use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Json},
    http::{header, Method, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use geo::{Area, Polygon as GeoPolygon};
use nalgebra::Point2;
use ordered_float::OrderedFloat;
use petgraph::graph::{Graph, NodeIndex};
use petgraph::visit::EdgeRef;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use tower_http::cors::{Any, CorsLayer};
use tracing::{info, warn};

mod graph_builder;
mod room_detector;

use graph_builder::*;
use room_detector::*;

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
}

fn default_area_threshold() -> f64 {
    100.0
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

    // Build graph from lines
    let graph = build_graph(&request.lines);
    info!("Built graph with {} nodes", graph.node_count());

    // Detect rooms (cycles)
    let rooms = detect_rooms(&graph, request.area_threshold);
    info!("Detected {} rooms", rooms.len());

    Ok(Json(DetectRoomsResponse {
        total_rooms: rooms.len(),
        rooms,
    }))
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

    // Configure CORS from environment or use localhost for development
    let allowed_origins = std::env::var("ALLOWED_ORIGINS")
        .unwrap_or_else(|_| "http://localhost:8080,http://127.0.0.1:8080".to_string());

    info!("Allowed CORS origins: {}", allowed_origins);

    let origins: Vec<_> = allowed_origins
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();

    let cors = if origins.is_empty() {
        // Fallback to Any only if no valid origins configured (not recommended for production)
        warn!("No valid CORS origins configured, allowing all origins (NOT SECURE FOR PRODUCTION)");
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

    // Build router with middleware
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/detect", post(detect_rooms_handler))
        .layer(DefaultBodyLimit::max(5 * 1024 * 1024)) // 5MB max request size
        .layer(cors);

    let addr = "0.0.0.0:3000";
    info!("Server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
