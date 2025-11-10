use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use image::{DynamicImage, GenericImageView, ImageFormat, ImageBuffer, Rgba, Luma, GrayImage};
use serde::{Deserialize, Serialize};
use std::io::Cursor;

/// Standard normalized coordinate space
pub const NORMALIZED_SIZE: u32 = 1000;

/// Normalized image with metadata for reverse mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedImage {
    /// Base64-encoded image (PNG format)
    pub base64_data: String,
    /// Original image dimensions (width, height)
    pub original_width: u32,
    pub original_height: u32,
    /// Scale factor applied during normalization
    pub scale_factor: f64,
    /// Padding applied (left, top, right, bottom)
    pub padding: (u32, u32, u32, u32),
}

/// Point in normalized coordinate space (0-1000)
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct NormalizedPoint {
    pub x: f64,
    pub y: f64,
}

/// Point in original image coordinate space
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct OriginalPoint {
    pub x: f64,
    pub y: f64,
}

impl NormalizedImage {
    /// Normalize an image from bytes to standard 1000x1000 coordinate space
    /// Preserves aspect ratio and pads with white background
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        // Decode image
        let img = image::load_from_memory(bytes)
            .context("Failed to decode image")?;

        let original_width = img.width();
        let original_height = img.height();

        // Calculate scale factor to fit within NORMALIZED_SIZE x NORMALIZED_SIZE
        let scale_x = NORMALIZED_SIZE as f64 / original_width as f64;
        let scale_y = NORMALIZED_SIZE as f64 / original_height as f64;
        let scale_factor = scale_x.min(scale_y);

        // Calculate new dimensions maintaining aspect ratio
        let new_width = (original_width as f64 * scale_factor) as u32;
        let new_height = (original_height as f64 * scale_factor) as u32;

        // Resize image
        let resized = img.resize_exact(
            new_width,
            new_height,
            image::imageops::FilterType::Lanczos3,
        );

        // Calculate padding to center the image
        let pad_x = (NORMALIZED_SIZE - new_width) / 2;
        let pad_y = (NORMALIZED_SIZE - new_height) / 2;

        // Create white background canvas
        let mut canvas = ImageBuffer::from_pixel(
            NORMALIZED_SIZE,
            NORMALIZED_SIZE,
            Rgba([255u8, 255u8, 255u8, 255u8]),
        );

        // Paste resized image onto canvas
        image::imageops::overlay(&mut canvas, &resized, pad_x as i64, pad_y as i64);

        // Convert to base64 PNG
        let normalized_img = DynamicImage::ImageRgba8(canvas);
        let mut png_bytes = Vec::new();
        normalized_img.write_to(
            &mut Cursor::new(&mut png_bytes),
            ImageFormat::Png,
        )?;

        let base64_data = STANDARD.encode(&png_bytes);

Ok(NormalizedImage {
    base64_data,
    original_width,
    original_height,
    scale_factor,
    padding: (pad_x, pad_y, pad_x, pad_y),
})
    }

    /// Convert normalized coordinates back to original image space
    pub fn denormalize_point(&self, normalized: NormalizedPoint) -> OriginalPoint {
        let (pad_left, pad_top, _, _) = self.padding;

        // Remove padding offset
        let x_after_padding = normalized.x - pad_left as f64;
        let y_after_padding = normalized.y - pad_top as f64;

        // Reverse scaling
        let original_x = x_after_padding / self.scale_factor;
        let original_y = y_after_padding / self.scale_factor;

        OriginalPoint {
            x: original_x,
            y: original_y,
        }
    }

    /// Convert multiple normalized points to original coordinate space
    pub fn denormalize_points(&self, points: &[NormalizedPoint]) -> Vec<OriginalPoint> {
        points.iter()
            .map(|&p| self.denormalize_point(p))
            .collect()
    }

    /// Convert original coordinates to normalized space
    pub fn normalize_point(&self, original: OriginalPoint) -> NormalizedPoint {
        let (pad_left, pad_top, _, _) = self.padding;

        // Apply scaling
        let x_scaled = original.x * self.scale_factor;
        let y_scaled = original.y * self.scale_factor;

        // Add padding offset
        let x_normalized = x_scaled + pad_left as f64;
        let y_normalized = y_scaled + pad_top as f64;

        NormalizedPoint {
            x: x_normalized,
            y: y_normalized,
        }
    }

    /// Convert multiple original points to normalized coordinate space
    pub fn normalize_points(&self, points: &[OriginalPoint]) -> Vec<NormalizedPoint> {
        points.iter()
            .map(|&p| self.normalize_point(p))
            .collect()
    }

/// Get the base64 data URL for use with vision APIs
pub fn to_data_url(&self) -> String {
    format!("data:image/png;base64,{}", self.base64_data)
}

/// Preprocess image for VTracer: create high-contrast binary image
/// VTracer needs strong contrast to detect lines
pub fn preprocess_for_vtracer(&self) -> anyhow::Result<Vec<u8>> {
    use std::io::Cursor;

    // Decode the normalized image
    let img_bytes = STANDARD.decode(&self.base64_data)?;
    let img = image::load_from_memory(&img_bytes)
        .context("Failed to load normalized image")?
        .to_luma8();

    // Simple threshold to create binary image
    // VTracer needs BLACK lines on WHITE background
    let threshold = 200u8;  // Adjust based on your blueprint images
    let binary = self.threshold_image(&img, threshold);

    // Convert to RGB (VTracer Color mode expects RGB)
    let (width, height) = binary.dimensions();
    let mut rgb_img = ImageBuffer::from_pixel(width, height, Rgba([255u8, 255u8, 255u8, 255u8]));

    for (x, y, pixel) in binary.enumerate_pixels() {
        let val = pixel[0];
        rgb_img.put_pixel(x, y, Rgba([val, val, val, 255u8]));
    }

    // Encode as PNG
    let mut png_bytes = Vec::new();
    DynamicImage::ImageRgba8(rgb_img).write_to(
        &mut Cursor::new(&mut png_bytes),
        ImageFormat::Png,
    )?;

    Ok(png_bytes)
}

fn sobel_edge_detect(&self, img: &GrayImage) -> GrayImage {
    let (width, height) = img.dimensions();
    let mut edges = GrayImage::new(width, height);
    
    for y in 1..height.saturating_sub(1) {
        for x in 1..width.saturating_sub(1) {
            // Sobel kernels for X and Y gradients
            let gx = -(img.get_pixel(x-1, y-1)[0] as i32) - 2 * (img.get_pixel(x-1, y)[0] as i32) - (img.get_pixel(x-1, y+1)[0] as i32)
                   + (img.get_pixel(x+1, y-1)[0] as i32) + 2 * (img.get_pixel(x+1, y)[0] as i32) + (img.get_pixel(x+1, y+1)[0] as i32);

            let gy = -(img.get_pixel(x-1, y-1)[0] as i32) - 2 * (img.get_pixel(x, y-1)[0] as i32) - (img.get_pixel(x+1, y-1)[0] as i32)
                   + (img.get_pixel(x-1, y+1)[0] as i32) + 2 * (img.get_pixel(x, y+1)[0] as i32) + (img.get_pixel(x+1, y+1)[0] as i32);
            
            let magnitude = ((gx * gx + gy * gy) as f32).sqrt().min(255.0) as u8;
            edges.put_pixel(x, y, Luma([magnitude]));
        }
    }
    
    edges
}

fn threshold_image(&self, img: &GrayImage, threshold: u8) -> GrayImage {
    let (width, height) = img.dimensions();
    let mut binary = GrayImage::new(width, height);

    for (x, y, pixel) in binary.enumerate_pixels_mut() {
        // VTracer expects BLACK lines on WHITE background
        // Pixels darker than threshold become BLACK (lines)
        // Pixels lighter than threshold become WHITE (background)
        let val = if img.get_pixel(x, y)[0] < threshold {
            0u8  // Dark pixel -> BLACK line
        } else {
            255u8  // Light pixel -> WHITE background
        };
        *pixel = Luma([val]);
    }

    binary
}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_square_image() {
        // Create a simple 500x500 white image
        let img = ImageBuffer::from_pixel(500, 500, Rgba([255u8, 255u8, 255u8, 255u8]));
        let mut bytes = Vec::new();
        DynamicImage::ImageRgba8(img)
            .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
            .unwrap();

        let normalized = NormalizedImage::from_bytes(&bytes).unwrap();

        // Square image should scale by 2.0
        assert_eq!(normalized.scale_factor, 2.0);
        assert_eq!(normalized.original_width, 500);
        assert_eq!(normalized.original_height, 500);
        assert_eq!(normalized.padding, (0, 0, 0, 0)); // No padding for square
    }

    #[test]
    fn test_normalize_rectangular_image() {
        // Create a 400x800 image (portrait)
        let img = ImageBuffer::from_pixel(400, 800, Rgba([255u8, 255u8, 255u8, 255u8]));
        let mut bytes = Vec::new();
        DynamicImage::ImageRgba8(img)
            .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
            .unwrap();

        let normalized = NormalizedImage::from_bytes(&bytes).unwrap();

        // Height is limiting dimension, scale = 1000/800 = 1.25
        assert_eq!(normalized.scale_factor, 1.25);
        assert_eq!(normalized.original_width, 400);
        assert_eq!(normalized.original_height, 800);

        // Width after scaling: 400 * 1.25 = 500
        // Padding: (1000 - 500) / 2 = 250 on each side
        assert_eq!(normalized.padding.0, 250); // left padding
        assert_eq!(normalized.padding.1, 0);   // top padding (height fills exactly)
    }

    #[test]
    fn test_coordinate_denormalization() {
        // Create a 500x500 image
        let img = ImageBuffer::from_pixel(500, 500, Rgba([255u8, 255u8, 255u8, 255u8]));
        let mut bytes = Vec::new();
        DynamicImage::ImageRgba8(img)
            .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
            .unwrap();

        let normalized = NormalizedImage::from_bytes(&bytes).unwrap();

        // Test point at normalized (500, 500) - center
        let norm_point = NormalizedPoint { x: 500.0, y: 500.0 };
        let orig_point = normalized.denormalize_point(norm_point);

        // Should map back to (250, 250) in original space
        assert!((orig_point.x - 250.0).abs() < 0.1);
        assert!((orig_point.y - 250.0).abs() < 0.1);
    }

    #[test]
    fn test_coordinate_normalization_denormalization_roundtrip() {
        // Create a 600x400 image
        let img = ImageBuffer::from_pixel(600, 400, Rgba([255u8, 255u8, 255u8, 255u8]));
        let mut bytes = Vec::new();
        DynamicImage::ImageRgba8(img)
            .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
            .unwrap();

        let normalized = NormalizedImage::from_bytes(&bytes).unwrap();

        // Test roundtrip: original -> normalized -> original
        let original = OriginalPoint { x: 300.0, y: 200.0 };
        let norm = normalized.normalize_point(original);
        let back_to_original = normalized.denormalize_point(norm);

        assert!((back_to_original.x - original.x).abs() < 0.1);
        assert!((back_to_original.y - original.y).abs() < 0.1);
    }

    #[test]
    fn test_base64_encoding() {
        let img = ImageBuffer::from_pixel(100, 100, Rgba([255u8, 255u8, 255u8, 255u8]));
        let mut bytes = Vec::new();
        DynamicImage::ImageRgba8(img)
            .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
            .unwrap();

        let normalized = NormalizedImage::from_bytes(&bytes).unwrap();

        // Verify base64 is not empty
        assert!(!normalized.base64_data.is_empty());

        // Verify data URL format
        let data_url = normalized.to_data_url();
        assert!(data_url.starts_with("data:image/png;base64,"));
    }
}