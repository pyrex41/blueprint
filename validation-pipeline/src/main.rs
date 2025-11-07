use aws_config::BehaviorVersion;
use aws_sdk_textract::{
    primitives::Blob,
    types::{Document, FeatureType},
    Client as TextractClient,
};
use hf_floorplan_loader::{FloorplanDataset, LoaderError};
use std::fs;
use std::path::Path;

mod textract_parser;
use textract_parser::*;

#[derive(Debug)]
struct ValidationResult {
    file_name: String,
    success: bool,
    extracted_lines_count: usize,
    error: Option<String>,
}

#[derive(Debug)]
struct ValidationReport {
    total_processed: usize,
    successful: usize,
    failed: usize,
    results: Vec<ValidationResult>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Starting Floorplan Validation Pipeline");
    println!("==========================================\n");

    // Load HuggingFace dataset
    println!("📁 Loading HuggingFace floorplan dataset...");
    let dataset = match FloorplanDataset::new() {
        Ok(ds) => ds,
        Err(e) => {
            eprintln!("❌ Failed to load dataset: {:?}", e);
            eprintln!("Make sure the HuggingFace dataset is downloaded to:");
            eprintln!("  ~/.cache/huggingface/hub/datasets--umesh16071973--New_Floorplan_demo_dataset/");
            return Ok(());
        }
    };

    println!("✅ Loaded {} floorplan images\n", dataset.len());

    // Initialize AWS Textract client
    println!("🔧 Initializing AWS Textract client...");
    let config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let textract_client = TextractClient::new(&config);
    println!("✅ AWS Textract client ready\n");

    // Process a subset of images (first 5 for demo)
    let sample_size = 5.min(dataset.len());
    println!("🔍 Processing first {} images...\n", sample_size);

    let mut report = ValidationReport {
        total_processed: 0,
        successful: 0,
        failed: 0,
        results: Vec::new(),
    };

    for (i, floorplan) in dataset.all().iter().take(sample_size).enumerate() {
        println!("Processing {}/{}: {}", i + 1, sample_size, floorplan.file_name);

        let result = process_floorplan(&textract_client, floorplan).await;

        match &result {
            Ok(lines) => {
                println!("  ✅ Extracted {} lines", lines.len());
                report.results.push(ValidationResult {
                    file_name: floorplan.file_name.clone(),
                    success: true,
                    extracted_lines_count: lines.len(),
                    error: None,
                });
                report.successful += 1;
            }
            Err(e) => {
                println!("  ❌ Failed: {}", e);
                report.results.push(ValidationResult {
                    file_name: floorplan.file_name.clone(),
                    success: false,
                    extracted_lines_count: 0,
                    error: Some(e.to_string()),
                });
                report.failed += 1;
            }
        }

        report.total_processed += 1;
        println!();
    }

    // Print summary report
    print_report(&report);

    Ok(())
}

async fn process_floorplan(
    client: &TextractClient,
    floorplan: &hf_floorplan_loader::FloorplanData,
) -> Result<Vec<Line>, String> {
    // Read image file
    let image_bytes = fs::read(&floorplan.image_path)
        .map_err(|e| format!("Failed to read image: {}", e))?;

    // Create document blob
    let document = Document::builder()
        .bytes(Blob::new(image_bytes))
        .build()
        .map_err(|e| format!("Failed to build document: {}", e))?;

    // Call Textract
    let response = client
        .analyze_document()
        .document(document)
        .feature_types(FeatureType::Layout)
        .send()
        .await
        .map_err(|e| format!("Textract API error: {}", e))?;

    // Parse response into lines
    let lines = parse_textract_response(response)?;

    Ok(lines)
}

fn print_report(report: &ValidationReport) {
    println!("==========================================");
    println!("📊 Validation Report");
    println!("==========================================\n");

    println!("Total Processed: {}", report.total_processed);
    println!("Successful: {} ({:.1}%)",
        report.successful,
        (report.successful as f64 / report.total_processed as f64) * 100.0
    );
    println!("Failed: {} ({:.1}%)",
        report.failed,
        (report.failed as f64 / report.total_processed as f64) * 100.0
    );

    println!("\n📋 Detailed Results:");
    println!("{:<15} {:<10} {:<15} {}", "File", "Status", "Lines", "Error");
    println!("{}", "-".repeat(70));

    for result in &report.results {
        let status = if result.success { "✅ OK" } else { "❌ FAIL" };
        let error = result.error.as_deref().unwrap_or("");
        println!(
            "{:<15} {:<10} {:<15} {}",
            result.file_name, status, result.extracted_lines_count, error
        );
    }

    println!("\n==========================================");
}

#[derive(Debug, Clone)]
pub struct Line {
    pub start: Point,
    pub end: Point,
}

#[derive(Debug, Clone)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}
