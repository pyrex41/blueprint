use opencv::{
    core,
    imgproc,
    imgcodecs,
    prelude::*,
    types,
};

use anyhow::{Context, Result};
use std::time::Instant;
use serde_json::json;
use crate::{Point, Room};

#[derive(Debug, Clone, Serialize)]
pub struct OpenCVRoom {
    pub id: usize,
    pub bounding_box: [f64; 4],
    pub area: f64,
    pub name_hint: String,
    pub points: Vec<Point>,
}

pub async fn detect_opencv_cc_handler(
    Json(request): Json<ImageDetectRequest>,
) -> Result<Json<DetectRoomsResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Received OpenCV connected components detection request");
    
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

    // Load image using OpenCV
    let img = imgcodecs::imdecode(
        &core::Mat::from_slice(&img_bytes).unwrap(),
        imgcodecs::IMREAD_COLOR
    ).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "INVALID_IMAGE".to_string(),
                message: format!("Failed to load image with OpenCV: {}", e),
            }),
        )
    })?;

    info!("Image loaded: {}x{}", img.cols(), img.rows());

    // Convert to grayscale
    let mut gray = core::Mat::default();
    imgproc::cvt_color(&img, &mut gray, imgproc::COLOR_BGR2GRAY, 0)?;

    // Apply Gaussian blur to reduce noise
    let mut blurred = core::Mat::default();
    imgproc::gaussian_blur(&gray, &mut blurred, core::Size::new(3, 3), 0.0, 0.0, core::BORDER_DEFAULT, core::Scalar::all(0.0))?;

    // Threshold to binary
    let mut binary = core::Mat::default();
    imgproc::threshold(&blurred, &mut binary, 128.0, 255.0, imgproc::THRESH_BINARY, core::no_array())?;

    // Find connected components
    let mut labels = core::Mat::default();
    let mut stats = core::Mat::default();
    let mut centroids = core::Mat::default();
    
    let num_labels = opencv::imgproc::connected_components_with_stats(
        &binary,
        &mut labels,
        &mut stats,
        &mut centroids,
        8, // 8-connectivity
        core::no_array(),
    )?;

    info!("Found {} connected components", num_labels);

    let mut rooms = Vec::new();
    let min_area = request.min_area as i32;
    let max_area_ratio = request.max_area_ratio;

    for i in 1..num_labels { // Skip background (label 0)
        let area = stats.at_2d(i as i32, types::CC_STAT_AREA)?;
        let bbox = stats.at_2d(i as i32, types::CC_STAT_LEFT | types::CC_STAT_TOP | types::CC_STAT_WIDTH | types::CC_STAT_HEIGHT)?;
        
        if *area > min_area {
            let x = bbox.at_2d(0, 0)?;
            let y = bbox.at_2d(0, 1)?;
            let w = bbox.at_2d(0, 2)?;
            let h = bbox.at_2d(0, 3)?;
            
            // Normalize to 0-1000 coordinate space
            let norm_x = (*x as f64 / img.cols() as f64) * 1000.0;
            let norm_y = (*y as f64 / img.rows() as f64) * 1000.0;
            let norm_w = (*w as f64 / img.cols() as f64) * 1000.0;
            let norm_h = (*h as f64 / img.rows() as f64) * 1000.0;
            
            let bounding_box = [norm_x, norm_y, norm_x + norm_w, norm_y + norm_h];
            
            // Filter by aspect ratio
            let aspect = (norm_w / norm_h).max(norm_h / norm_w);
            if aspect < 15.0 {
                rooms.push(OpenCVRoom {
                    id: i as usize,
                    bounding_box,
                    area: *area as f64,
                    name_hint: "Room".to_string(),
                    points: Vec::new(),
                });
            }
        }
    }

    let execution_time = start_time.elapsed().as_millis() as u64;
    info!("Detected {} rooms using OpenCV in {}ms", rooms.len(), execution_time);

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
