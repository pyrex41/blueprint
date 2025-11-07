use axum::{
    extract::Json,
    http::{header, Method, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use geo::{Area, Polygon as GeoPolygon};
use nalgebra::Point2;
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
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
        (self.x - other.x).abs() < 1e-6 && (self.y - other.y).abs() < 1e-6
    }
}

impl Eq for Point {}

impl std::hash::Hash for Point {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Hash as integers to avoid floating point precision issues
        ((self.x * 1_000_000.0) as i64).hash(state);
        ((self.y * 1_000_000.0) as i64).hash(state);
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
) -> Result<Json<DetectRoomsResponse>, StatusCode> {
    info!("Received detection request with {} lines", request.lines.len());

    if request.lines.is_empty() {
        warn!("Empty lines input");
        return Ok(Json(DetectRoomsResponse {
            rooms: vec![],
            total_rooms: 0,
        }));
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

    // Configure CORS
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([header::CONTENT_TYPE]);

    // Build router
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/detect", post(detect_rooms_handler))
        .layer(cors);

    let addr = "0.0.0.0:3000";
    info!("Server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
