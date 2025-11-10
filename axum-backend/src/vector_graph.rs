use std::collections::{HashMap, VecDeque};
use std::time::Instant;
use std::path::Path;
use regex::Regex;
use serde_json::json;
use axum::{
    extract::Json,
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use tracing::info;

use base64::Engine;
use anyhow::{Result, Context};
use ordered_float::OrderedFloat;
use crate::{Point, Room, ImageDetectRequest, DetectRoomsResponse, ErrorResponse};

#[derive(Debug, Clone)]
pub struct LineSegment {
    pub start: Point,
    pub end: Point,
}

pub async fn detect_vector_graph_handler(
    Json(request): Json<ImageDetectRequest>,
) -> Result<Json<DetectRoomsResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Received vector graph detection request");
    
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

    let start_time = Instant::now();

    // Step 1: Convert image to SVG using VTracer
    let temp_svg_path = std::env::temp_dir().join("temp_vector_graph.svg");
    let temp_svg = temp_svg_path.as_path();
    
    convert_image_to_svg_vtracer(&img_bytes, temp_svg)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "VECTORIZATION_FAILED".to_string(),
                    message: format!("VTracer conversion failed: {}", e),
                }),
            )
        })?;
    
    // Step 2: Read SVG content
    let svg_content = tokio::fs::read_to_string(temp_svg)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "SVG_READ_FAILED".to_string(),
                    message: format!("Failed to read SVG: {}", e),
                }),
            )
        })?;
    
    // Cleanup temp file
    let _ = tokio::fs::remove_file(temp_svg).await;
    
    // Step 3: Parse SVG to line segments
    let segments = parse_svg_to_lines_vtracer(&svg_content)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "SVG_PARSE_FAILED".to_string(),
                    message: format!("Failed to parse SVG: {}", e),
                }),
            )
        })?;
    
    info!("Parsed {} line segments from SVG", segments.len());
    
    // Step 4: Find vertices and build graph
    let vertices = find_intersections_vtracer(&segments);
    let graph = build_graph_vtracer(&vertices, &segments);
    
    info!("Graph built with {} nodes", graph.len());
    
    // Step 5: Find cycles and filter rooms
    let cycles = all_cycles_vtracer(&graph);
    let mut minimal_cycles = Vec::new();
    
    for cycle in cycles {
        if is_minimal_cycle_vtracer(&cycle, &vertices, &graph) {
            minimal_cycles.push(cycle);
        }
    }
    
    info!("Found {} minimal cycles (potential rooms)", minimal_cycles.len());
    
    // Step 6: Generate rooms
    let mut rooms = Vec::new();
    for (i, cycle) in minimal_cycles.iter().enumerate() {
        let points: Vec<Point> = cycle.iter().map(|&idx| vertices[idx].clone()).collect();
        let bbox = compute_bounding_box_vtracer(&points);
        let area = compute_area_vtracer(&points);
        
        if area > 100.0 {
            rooms.push(Room {
                id: i,
                bounding_box: bbox,
                area,
                name_hint: "Room".to_string(),
                points,
            });
        }
    }
    
    let execution_time = start_time.elapsed().as_millis() as u64;
    info!("Detected {} rooms using vector graph in {}ms", rooms.len(), execution_time);
    
    Ok(Json(DetectRoomsResponse {
        total_rooms: rooms.len(),
        rooms,
    }))
}

async fn convert_image_to_svg_vtracer(img_bytes: &[u8], output_path: &Path) -> Result<()> {
    use vtracer::{Config, Hierarchical, ColorMode};
    
    // Save image to temp file for VTracer
    let temp_image = std::env::temp_dir().join("temp_vtracer_input.png");
    tokio::fs::write(&temp_image, img_bytes).await?;
    
    let config = Config {
        color_mode: ColorMode::Binary,
        hierarchical: Hierarchical::Stacked,
        mode: visioncortex::PathSimplifyMode::Spline,
        filter_speckle: 2,
        color_precision: 4,
        layer_difference: 8,
        corner_threshold: 50,
        length_threshold: 2.0,
        max_iterations: 15,
        splice_threshold: 30,
        path_precision: Some(2),
    };
    
    vtracer::convert_image_to_svg(&temp_image, output_path, config)
        .map_err(|e| anyhow::anyhow!("VTracer conversion failed: {}", e))?;
    
    // Cleanup
    let _ = tokio::fs::remove_file(&temp_image).await;
    
    Ok(())
}

fn parse_svg_to_lines_vtracer(svg_content: &str) -> Result<Vec<LineSegment>> {
    use regex::Regex;
    
    let mut segments = Vec::new();
    
    // Simple regex to find path d attributes
    let path_regex = Regex::new(r#"d="([^"]*)"#).unwrap();
    let path_matches = path_regex.captures_iter(svg_content);
    
    for cap in path_matches {
        let d = &cap[1];
        let tokens: Vec<&str> = d.split(|c| c == ' ' || c == ',' || c == 'M' || c == 'L' || c == 'Z').filter(|s| !s.is_empty()).collect();
        
        let mut i = 0;
        let mut current_pos = Point { x: 0.0, y: 0.0 };
        let mut start_pos = Point { x: 0.0, y: 0.0 };
        let mut in_line = false;
        
        while i < tokens.len() {
            let token = tokens[i];
            i += 1;
            
            if token == "M" {
                in_line = false;
                if i + 1 < tokens.len() {
                    let x = tokens[i].parse().unwrap_or(0.0);
                    let y = tokens[i + 1].parse().unwrap_or(0.0);
                    current_pos = Point { x, y };
                    start_pos = Point { x, y };
                    i += 2;
                }
            } else if token == "L" {
                in_line = true;
            } else if token == "Z" {
                if in_line && current_pos != start_pos {
                    segments.push(LineSegment {
                        start: current_pos.clone(),
                        end: start_pos.clone(),
                    });
                }
                in_line = false;
            } else if in_line {
                if i < tokens.len() {
                    let x = token.parse::<f64>().unwrap_or(current_pos.x);
                    let y = tokens[i].parse::<f64>().unwrap_or(current_pos.clone().y);
                    segments.push(LineSegment {
                        start: current_pos,
                        end: Point { x, y },
                    });
                    current_pos = Point { x, y };
                    i += 1;
                }
            } else if let Ok(num) = token.parse::<f64>() {
                if i < tokens.len() {
                    let y = tokens[i].parse::<f64>().unwrap_or(0.0);
                    segments.push(LineSegment {
                        start: current_pos,
                        end: Point { x: num, y },
                    });
                    current_pos = Point { x: num, y };
                    i += 1;
                }
            }
        }
    }
    
    Ok(segments)
}

fn find_intersections_vtracer(segments: &[LineSegment]) -> Vec<Point> {
    let mut vertices = Vec::new();
    
    // Add all endpoints
    for seg in segments {
        vertices.push(seg.start.clone());
        vertices.push(seg.end.clone());
    }
    
    // Find intersections between segments
    for i in 0..segments.len() {
        for j in (i + 1)..segments.len() {
            if let Some(inter) = line_intersection_vtracer(&segments[i], &segments[j]) {
                vertices.push(inter);
            }
        }
    }
    
    // Remove duplicates
    let mut unique_vertices: Vec<Point> = Vec::new();
    for v in vertices {
        let mut is_duplicate = false;
        for existing in &unique_vertices {
            if (v.x - existing.x).abs() < 1e-6 && (v.y - existing.y).abs() < 1e-6 {
                is_duplicate = true;
                break;
            }
        }
        if !is_duplicate {
            unique_vertices.push(v);
        }
    }
    
    unique_vertices
}

fn line_intersection_vtracer(seg1: &LineSegment, seg2: &LineSegment) -> Option<Point> {
    let (x1, y1) = (seg1.start.x, seg1.start.y);
    let (x2, y2) = (seg1.end.x, seg1.end.y);
    let (x3, y3) = (seg2.start.x, seg2.start.y);
    let (x4, y4) = (seg2.end.x, seg2.end.y);
    
    let denom = (y4 - y3) * (x2 - x1) - (x4 - x3) * (y2 - y1);
    if denom.abs() < 1e-10 {
        return None;
    }
    
    let t = ((x1 - x3) * (y4 - y3) - (y1 - y3) * (x4 - x3)) / denom;
    let u = -((y4 - y3) * (x1 - x3) - (x4 - x3) * (y1 - y3)) / denom;
    
    if t >= 0.0 && t <= 1.0 && u >= 0.0 && u <= 1.0 {
        Some(Point {
            x: x1 + t * (x2 - x1),
            y: y1 + t * (y2 - y1),
        })
    } else {
        None
    }
}

fn build_graph_vtracer(vertices: &[Point], segments: &[LineSegment]) -> HashMap<usize, Vec<usize>> {
    let mut point_to_idx: HashMap<(OrderedFloat<f64>, OrderedFloat<f64>), usize> = HashMap::new();
    for (i, v) in vertices.iter().enumerate() {
        let key = (OrderedFloat(v.x), OrderedFloat(v.y));
        point_to_idx.insert(key, i);
    }
    
    let mut graph: HashMap<usize, Vec<usize>> = HashMap::new();
    
    for seg in segments {
        let start_key = (OrderedFloat(seg.start.x), OrderedFloat(seg.start.y));
        let end_key = (OrderedFloat(seg.end.x), OrderedFloat(seg.end.y));

        if let (Some(&i1), Some(&i2)) = (point_to_idx.get(&start_key), point_to_idx.get(&end_key)) {
            if i1 != i2 {
                graph.entry(i1).or_insert_with(Vec::new).push(i2);
                graph.entry(i2).or_insert_with(Vec::new).push(i1);
            }
        }
    }
    
    graph
}

fn find_cycles_vtracer(graph: &HashMap<usize, Vec<usize>>, start: usize) -> Vec<Vec<usize>> {
    let mut cycles = Vec::new();
    let mut stack = vec![(start, vec![start])];
    
    while let Some((node, path)) = stack.pop() {
        for &neighbor in graph.get(&node).unwrap_or(&Vec::new()) {
            if neighbor == start && path.len() > 2 {
                cycles.push(path.clone());
            } else if !path.contains(&neighbor) {
                let mut new_path = path.clone();
                new_path.push(neighbor);
                stack.push((neighbor, new_path));
            }
        }
    }
    
    cycles
}

fn all_cycles_vtracer(graph: &HashMap<usize, Vec<usize>>) -> Vec<Vec<usize>> {
    let mut all_cycles = Vec::new();
    let mut visited_starts = std::collections::HashSet::new();
    
    for &start in graph.keys() {
        if !visited_starts.contains(&start) {
            let cycles = find_cycles_vtracer(graph, start);
            for cycle in cycles {
                let mut sorted_cycle = cycle.clone();
                sorted_cycle.sort();
                let cycle_tuple: Vec<_> = sorted_cycle.iter().map(|&x| x).collect();
                
                if !all_cycles.iter().any(|c: &Vec<usize>| {
                    let mut sorted_c = c.clone();
                    sorted_c.sort();
                    sorted_c == cycle_tuple
                }) {
                    all_cycles.push(cycle.clone());
                }
            }
            visited_starts.insert(start);
        }
    }
    
    all_cycles
}

fn point_in_polygon_vtracer(point: &Point, poly: &[Point]) -> bool {
    let (x, y) = (point.x, point.y);
    let n = poly.len();
    let mut inside = false;
    let mut p1x = poly[0].x;
    let mut p1y = poly[0].y;
    
    for i in 0..n {
        let p2x = poly[(i + 1) % n].x;
        let p2y = poly[(i + 1) % n].y;
        
        if y > p1y.min(p2y) {
            if y <= p1y.max(p2y) {
                if x <= p1x.max(p2x) {
                    if p1y != p2y {
                        let xinters = (y - p1y) * (p2x - p1x) / (p2y - p1y) + p1x;
                        if p1x == p2x || x <= xinters {
                            inside = !inside;
                        }
                    }
                }
            }
        }
        p1x = p2x;
        p1y = p2y;
    }
    
    inside
}

fn is_minimal_cycle_vtracer(cycle: &[usize], vertices: &[Point], graph: &HashMap<usize, Vec<usize>>) -> bool {
    if cycle.len() < 3 {
        return false;
    }
    
    let poly_points: Vec<Point> = cycle.iter().map(|&i| vertices[i].clone()).collect();
    
    for (i, v) in vertices.iter().enumerate() {
        if !cycle.contains(&i) {
            if point_in_polygon_vtracer(v, &poly_points) {
                return false;
            }
        }
    }
    
    true
}

fn compute_bounding_box_vtracer(points: &[Point]) -> [f64; 4] {
    if points.is_empty() {
        return [0.0, 0.0, 0.0, 0.0];
    }
    
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    
    for p in points {
        min_x = min_x.min(p.x);
        min_y = min_y.min(p.y);
        max_x = max_x.max(p.x);
        max_y = max_y.max(p.y);
    }
    
    [min_x, min_y, max_x, max_y]
}

fn compute_area_vtracer(points: &[Point]) -> f64 {
    if points.len() < 3 {
        return 0.0;
    }
    
    let mut area = 0.0;
    let n = points.len();
    
    for i in 0..n {
        let j = (i + 1) % n;
        area += points[i].x * points[j].y;
        area -= points[j].x * points[i].y;
    }
    
    area.abs() / 2.0
}