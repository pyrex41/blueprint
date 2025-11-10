use reqwest;
use leptos::prelude::*;
use serde::Serialize;
use crate::{DetectResponse, Room, Point, Line};

#[derive(Serialize)]
struct RustFloodFillRequest {
    image: String,
    threshold: u8,
    min_area: usize,
    max_area_ratio: f32,
}

#[derive(Serialize)]
struct VectorGraphRequest {
    image: String,
}

async fn detect_rust_floodfill(image: String, threshold: u8, min_area: usize, max_area_ratio: f32) -> Result<DetectResponse, String> {
    let client = reqwest::Client::new();

    let request = RustFloodFillRequest { 
        image, 
        threshold, 
        min_area, 
        max_area_ratio 
    };

    let response = client
        .post("http://localhost:3000/detect/rust-floodfill")
        .json(&request)
        .send()
        .await
        .map_err(|e| format!("Rust Flood Fill request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(format!("Server error ({}): {}", status, error_text));
    }

    response
        .json::<DetectResponse>()
        .await
        .map_err(|e| format!("Failed to parse Rust Flood Fill response: {}", e))
}

async fn detect_vector_graph(image: String) -> Result<DetectResponse, String> {
    let client = reqwest::Client::new();

    let request = VectorGraphRequest { image };

    let response = client
        .post("http://localhost:3000/detect/vector-graph")
        .json(&request)
        .send()
        .await
        .map_err(|e| format!("Vector Graph request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(format!("Server error ({}): {}", status, error_text));
    }

    response
        .json::<DetectResponse>()
        .await
        .map_err(|e| format!("Failed to parse Vector Graph response: {}", e))
}