use image::{GrayImage, Luma};
use std::collections::VecDeque;
use crate::Room;

fn threshold_image(img: &GrayImage, threshold: u8) -> GrayImage {
    let mut binary = GrayImage::new(img.width(), img.height());
    for (x, y, pixel) in img.enumerate_pixels() {
        let val = if pixel[0] > threshold { 255 } else { 0 };
        binary.put_pixel(x, y, Luma([val]));
    }
    binary
}

fn find_connected_components(
    img: &GrayImage,
    min_area: usize,
    max_area: f32,
) -> Vec<(usize, (u32, u32, u32, u32))> {
    let (width, height) = img.dimensions();
    let mut visited = vec![false; width as usize * height as usize];
    let mut components = Vec::new();

    for y in 0..height {
        for x in 0..width {
            let idx = (y as usize * width as usize) + x as usize;
            if img.get_pixel(x, y)[0] == 255 && !visited[idx] {
                let (area, bbox) = flood_fill(img, x, y, &mut visited, width, height);
                let (min_x, min_y, max_x, max_y) = bbox;

                // Calculate dimensions
                let w = max_x - min_x;
                let h = max_y - min_y;

                // Calculate aspect ratio (width/height or height/width, whichever is larger)
                let aspect_ratio = if w > h {
                    w as f64 / h.max(1) as f64
                } else {
                    h as f64 / w.max(1) as f64
                };

                // Filter by area and aspect ratio
                // Reject thin elongated shapes (aspect ratio > 15) which are likely walls
                if area >= min_area && (area as f32) < max_area && aspect_ratio < 15.0 {
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

/// Detect rooms using connected components analysis on a binary image
use imageproc::morphology::{dilate, erode};
use imageproc::distance_transform::Norm;
use image::imageops::invert;

pub fn detect_rooms_connected_components(
    img: &GrayImage,
    threshold: u8,
    min_area: usize,
    max_area_ratio: f32,
) -> Vec<Room> {
    // Simple threshold - no morphological operations (like Algorithm 1)
    let binary = threshold_image(img, threshold);

    // Basic absolute thresholds
    let max_area = (img.width() as usize * img.height() as usize) * 3 / 10; // 30% of image
    let min_area = 500;

    // Find connected components
    let components = find_connected_components(&binary, min_area, max_area as f32);

    // Convert components to rooms
    let mut rooms = Vec::new();

    // Find the largest component for relative filtering (same as Algorithm 1)
    let max_component_area = components.iter().map(|(area, _)| *area).max().unwrap_or(0);
    let relative_threshold = (max_component_area as f64 * 0.05) as usize; // 5% of largest (same as Algorithm 1)

    let mut room_id = 1;
    for (area, bbox) in components.iter() {
        // Apply both absolute and relative size filtering (like Algorithm 1)
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

        // More lenient aspect ratio like Algorithm 1
        if aspect_ratio > 8.0 {
            continue;
        }

        // No fill ratio check (removed)

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
            name_hint: generate_room_name(*area as f64),
            points,
        });

        room_id += 1;
    }

    // Sort rooms by area (largest first) to maintain consistent ordering
    rooms.sort_by(|a, b| b.area.partial_cmp(&a.area).unwrap_or(std::cmp::Ordering::Equal));

    rooms
}

pub fn threshold_image_enhanced(img: &GrayImage, threshold: u8) -> GrayImage {
    let mut binary = GrayImage::new(img.width(), img.height());
    for (x, y, pixel) in img.enumerate_pixels() {
        let val = if pixel[0] > threshold { 255 } else { 0 };
        binary.put_pixel(x, y, Luma([val]));
    }
    binary
}

pub fn find_connected_components_enhanced(
    img: &GrayImage,
    min_area: usize,
    max_area: f32,
) -> Vec<(usize, (u32, u32, u32, u32))> {
    let (width, height) = img.dimensions();
    let mut visited = vec![false; (width as usize * height as usize)];
    let mut components = Vec::new();

    for y in 0..height {
        for x in 0..width {
            let idx = (y as usize * width as usize) + x as usize;
            if img.get_pixel(x, y)[0] == 255 && !visited[idx] {
                let (area, bbox) = flood_fill_enhanced(img, x, y, &mut visited, width, height);
                let (min_x, min_y, max_x, max_y) = bbox;

                // Calculate dimensions
                let w = max_x - min_x;
                let h = max_y - min_y;

                // Calculate aspect ratio (width/height or height/width, whichever is larger)
                let aspect_ratio = if w > h {
                    w as f64 / h.max(1) as f64
                } else {
                    h as f64 / w.max(1) as f64
                };

                // Filter by area and aspect ratio
                // Reject thin elongated shapes (aspect ratio > 10) which are likely walls
                if area >= min_area && (area as f32) < max_area && aspect_ratio < 15.0 {
                    components.push((area, bbox));
                }
            }
        }
    }
    components
}

fn flood_fill_enhanced(
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

pub fn generate_room_name(area: f64) -> String {
    if area < 5000.0 {
        "Small Room".to_string()
    } else if area < 20000.0 {
        "Medium Room".to_string()
    } else {
        "Large Room".to_string()
    }
}


