use axum::{extract::Json, http::StatusCode};
use base64::Engine;
use image::{GrayImage, Luma};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::Instant;
use tracing::info;

use crate::{ErrorResponse, Point, Room};

#[derive(Debug, Deserialize)]
pub struct ImageDetectRequest {
    pub image: String,
    pub threshold: u8,
    pub min_area: usize,
    pub max_area_ratio: f32,
}

#[derive(Debug, Serialize)]
pub struct DetectRoomsResponse {
    pub total_rooms: usize,
    pub rooms: Vec<Room>,
}

#[derive(Debug)]
struct FloodFillRoom {
    id: usize,
    bounding_box: [f64; 4],
    area: f64,
    name_hint: String,
    points: Vec<Point>,
}

// Morphological operations not needed for this simpler approach

pub async fn detect_rust_floodfill_handler(
    Json(request): Json<ImageDetectRequest>,
) -> Result<Json<DetectRoomsResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Received Rust flood fill detection request");

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

    // Simple threshold - no morphological operations
    let binary = threshold_image_floodfill(&img, request.threshold);

    // Find components
    let components = find_connected_components_floodfill(&binary);

    let (width, height) = img.dimensions();

    // Basic absolute thresholds
    let max_area = (width as usize * height as usize) * 3 / 10; // 30% of image
    let min_area = 500;

    // Find the largest component for relative filtering
    let max_component_area = components.iter().map(|(area, _)| *area).max().unwrap_or(0);
    let relative_threshold = (max_component_area as f64 * 0.05) as usize; // 5% of largest

    let mut rooms = Vec::new();
    let mut room_id = 1;

    for (area, bbox) in components.iter() {
        // Apply both absolute and relative size filtering (like the working Python version)
        if *area < min_area || *area > max_area || *area < relative_threshold {
            continue;
        }

        let (min_x, min_y, max_x, max_y) = *bbox;
        let w = max_x - min_x;
        let h = max_y - min_y;

        // Calculate aspect ratio
        let aspect_ratio = if w > h {
            w as f64 / h.max(1) as f64
        } else {
            h as f64 / w.max(1) as f64
        };

        // More lenient aspect ratio like Python version
        if aspect_ratio > 8.0 {
            continue;
        }

        // No fill ratio check (removed)

        let bbox_norm = normalize_bbox_floodfill(*bbox, width, height);

        rooms.push(FloodFillRoom {
            id: room_id,
            bounding_box: bbox_norm,
            area: *area as f64,
            name_hint: "Room".to_string(),
            points: Vec::new(),
        });

        room_id += 1;
    }

    let execution_time = start_time.elapsed().as_millis() as u64;
    info!("Detected {} rooms using Rust flood fill in {}ms", rooms.len(), execution_time);

    Ok(Json(DetectRoomsResponse {
        total_rooms: rooms.len(),
        rooms: rooms.into_iter().map(|r| Room {
            id: r.id,
            bounding_box: r.bounding_box,
            area: r.area,
            name_hint: r.name_hint,
            points: r.points,
        }).collect(),
    }))
}

fn threshold_image_floodfill(img: &GrayImage, threshold: u8) -> GrayImage {
    let mut binary = GrayImage::new(img.width(), img.height());
    for (x, y, pixel) in img.enumerate_pixels() {
        let val = if pixel[0] > threshold { 255 } else { 0 };
        binary.put_pixel(x, y, Luma([val]));
    }

    binary
}

fn find_connected_components_floodfill(img: &GrayImage) -> Vec<(usize, (u32, u32, u32, u32))> {
    let (width, height) = img.dimensions();
    let mut visited = vec![false; width as usize * height as usize];
    let mut components = Vec::new();

    // Early filters (same as Algorithm 2)
    let min_area = 500;
    let max_area = (width as usize * height as usize) * 3 / 10; // 30% of image

    for y in 0..height {
        for x in 0..width {
            let idx = (y as usize * width as usize) + x as usize;
            if img.get_pixel(x, y)[0] == 255 && !visited[idx] {
                let (area, bbox) = flood_fill_internal(img, x, y, &mut visited, width, height);
                let (min_x, min_y, max_x, max_y) = bbox;

                // Calculate dimensions
                let w = max_x - min_x;
                let h = max_y - min_y;

                // Calculate aspect ratio
                let aspect_ratio = if w > h {
                    w as f64 / h.max(1) as f64
                } else {
                    h as f64 / w.max(1) as f64
                };

                // Early filter by area and aspect ratio (same as Algorithm 2)
                if area >= min_area && area <= max_area && aspect_ratio < 15.0 {
                    components.push((area, bbox));
                }
            }
        }
    }
    components
}

fn flood_fill_internal(
    img: &GrayImage,
    start_x: u32,
    start_y: u32,
    visited: &mut Vec<bool>,
    width: u32,
    height: u32,
) -> (usize, (u32, u32, u32, u32)) {
    let mut queue = VecDeque::new();
    let mut area = 0;
    let mut min_x = start_x;
    let mut min_y = start_y;
    let mut max_x = start_x;
    let mut max_y = start_y;

    queue.push_back((start_x, start_y));
    let start_idx = (start_y as usize * width as usize) + start_x as usize;
    visited[start_idx] = true;

    while let Some((x, y)) = queue.pop_front() {
        area += 1;
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);

        // 8-connectivity neighbors
        for dx in -1..=1 {
            for dy in -1..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                if nx >= 0 && nx < width as i32 && ny >= 0 && ny < height as i32 {
                    let px = nx as u32;
                    let py = ny as u32;
                    let nidx = (py as usize * width as usize) + px as usize;
                    if !visited[nidx] && img.get_pixel(px, py)[0] == 255 {
                        visited[nidx] = true;
                        queue.push_back((px, py));
                    }
                }
            }
        }
    }

    (area, (min_x, min_y, max_x, max_y))
}

fn normalize_bbox_floodfill(bbox: (u32, u32, u32, u32), width: u32, height: u32) -> [f64; 4] {
    let (min_x, min_y, max_x, max_y) = bbox;
    [
        (min_x as f64 / width as f64) * 1000.0,
        (min_y as f64 / height as f64) * 1000.0,
        (max_x as f64 / width as f64) * 1000.0,
        (max_y as f64 / height as f64) * 1000.0,
    ]
}
