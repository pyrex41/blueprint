use anyhow::{Context, Result};
use image::{GrayImage, Luma};
use imageproc::morphology::{dilate, erode};
use std::collections::VecDeque;
use std::env;
use std::fs;
use std::path::Path;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Room {
    pub id: usize,
    pub bounding_box: [f64; 4],
    pub area: f64,
    pub name_hint: String,
    pub points: Vec<Point>,
}

fn load_image(path: &Path) -> Result<GrayImage> {
    let img = image::open(path)
        .with_context(|| format!("Failed to load image: {}", path.display()))?
        .to_luma8();
    Ok(img)
}

fn threshold_image(img: &GrayImage, threshold: u8) -> GrayImage {
    let mut binary = GrayImage::new(img.width(), img.height());
    for (x, y, pixel) in img.enumerate_pixels() {
        let val = if pixel[0] > threshold { 255 } else { 0 };
        binary.put_pixel(*x, *y, image::Luma([val]));
    }
    
    // Apply morphological closing to connect broken walls
    let kernel = imageproc::kernels::horizontal_vertical::MORPH_KERNEL_3X3;
    let dilated = dilate(&binary, kernel);
    let binary = erode(&dilated, kernel);
    
    binary
}

fn find_connected_components(img: &GrayImage) -> Vec<(usize, (u32, u32, u32, u32))> {
    let (width, height) = img.dimensions();
    let mut visited = vec![false; (width as usize * height as usize)];
    let mut components = Vec::new();
    
    for y in 0..height {
        for x in 0..width {
            let idx = (y as usize * width as usize) + x as usize;
            if img.get_pixel(x, y)[0] == 255 && !visited[idx] {
                let (area, bbox) = flood_fill(img, x, y, &mut visited, width, height);
                if area > 500 {
                    components.push((area, bbox));
                }
            }
        }
    }
    components
}

fn flood_fill(
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

fn normalize_bbox(bbox: (u32, u32, u32, u32), width: u32, height: u32) -> [f64; 4] {
    let (min_x, min_y, max_x, max_y) = bbox;
    let norm_x = (min_x as f64 / width as f64) * 1000.0;
    let norm_y = (min_y as f64 / height as f64) * 1000.0;
    let norm_w = ((max_x - min_x) as f64 / width as f64) * 1000.0;
    let norm_h = ((max_y - min_y) as f64 / height as f64) * 1000.0;
    [norm_x, norm_y, norm_x + norm_w, norm_y + norm_h]
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let image_path = if args.len() > 1 {
        args[1].clone()
    } else {
        "test-data/test_blueprint_004.png".to_string()
    };
    
    println!("Enhanced Connected Components Detection");
    println!("Loading image: {}", image_path);
    
    let img = load_image(Path::new(&image_path))?;
    let (width, height) = img.dimensions();
    
    // Threshold and clean
    let binary = threshold_image(&img, 128);
    
    // Find components
    let components = find_connected_components(&binary);
    
    let max_area = (width as usize * height as usize) / 5;
    let min_area = 500;
    
    let mut rooms = Vec::new();
    for (i, (area, bbox)) in components.iter().enumerate() {
        if *area > min_area && *area < max_area {
            let bbox_norm = normalize_bbox(*bbox, width, height);
            let w = bbox.2 - bbox.0;
            let h = bbox.3 - bbox.1;
            let aspect = (w as f64 / h as f64).max(h as f64 / w as f64);
            
            if aspect < 15.0 {
                rooms.push(Room {
                    id: i,
                    bounding_box: bbox_norm,
                    area: *area as f64,
                    name_hint: "Room".to_string(),
                    points: Vec::new(), // Bounding box only
                });
            }
        }
    }
    
    // Output JSON
    let output = json!({
        "rooms": rooms,
        "total_rooms": rooms.len(),
        "image_size": [width as f64, height as f64],
        "components_found": components.len()
    });
    
    let json_str = serde_json::to_string_pretty(&output)?;
    fs::write("detected_rooms_enhanced_cc.json", json_str)?;
    
    println!("Detected {} rooms", rooms.len());
    println!("Output saved to detected_rooms_enhanced_cc.json");
    
    Ok(())
}