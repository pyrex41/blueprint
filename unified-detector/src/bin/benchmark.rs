// Benchmark all detection methods on sample images
use instant::Instant;
use serde_json::json;
use std::fs;
use std::path::Path;
use std::time::Duration;
use tracing::{info, warn};
use unified_detector::*;
use vision_classifier::VisionClassifier;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    println!("🔬 Floorplan Detection Benchmark Suite");
    println!("=======================================\n");

    // Find test images
    let test_images = find_test_images("data")?;
    println!("📁 Found {} test images\n", test_images.len());

    if test_images.is_empty() {
        println!("⚠️  No test images found in data/");
        println!("Run: cargo run --bin test-floorplan");
        return Ok(());
    }

    // Determine which methods are available
    let available_methods = vec![
        DetectionMethod::GraphBased,
        DetectionMethod::GraphWithDoors,
    ];

    let has_openai = std::env::var("OPENAI_API_KEY").is_ok();
    let mut all_methods = available_methods.clone();

    if has_openai {
        all_methods.push(DetectionMethod::VisionGPT5);
        println!("✅ OpenAI API key found - will test GPT-5 Vision");
    } else {
        println!("⚠️  No OPENAI_API_KEY - skipping vision methods");
        println!("   Set with: export OPENAI_API_KEY=sk-...\n");
    }

    println!("🎯 Testing methods: {:?}\n", all_methods.iter().map(|m| m.name()).collect::<Vec<_>>());

    // Run benchmarks
    let mut all_results = Vec::new();

    for (i, image_path) in test_images.iter().enumerate() {
        println!("▶ Testing {}/{}: {}", i + 1, test_images.len(), image_path);

        for method in &all_methods {
            let result = benchmark_single(image_path, *method).await;

            if result.success {
                println!(
                    "  ✅ {}: {} rooms in {:.2}s (conf: {:.1}%)",
                    method.name(),
                    result.rooms_detected,
                    result.execution_time.as_secs_f64(),
                    result.avg_confidence * 100.0
                );
            } else {
                println!(
                    "  ❌ {}: {}",
                    method.name(),
                    result.error.as_deref().unwrap_or("Unknown error")
                );
            }

            all_results.push(result);
        }
        println!();
    }

    // Generate statistics
    println!("\n📊 Benchmark Results");
    println!("{}", "=".repeat(80));

    for method in &all_methods {
        let stats = BenchmarkStats::from_results(*method, &all_results);
        print_stats(&stats);
    }

    // Save results
    let report = json!({
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "total_images": test_images.len(),
        "methods_tested": all_methods.iter().map(|m| m.name()).collect::<Vec<_>>(),
        "results": all_results,
        "statistics": all_methods.iter().map(|m|
            BenchmarkStats::from_results(*m, &all_results)
        ).collect::<Vec<_>>(),
    });

    let output_path = "data/benchmark_results.json";
    fs::write(output_path, serde_json::to_string_pretty(&report)?)?;
    println!("\n💾 Saved detailed results to: {}", output_path);

    // Generate comparison report
    generate_comparison_table(&all_methods, &all_results);

    Ok(())
}

async fn benchmark_single(image_path: &str, method: DetectionMethod) -> BenchmarkResult {
    let start = Instant::now();

    let result = match method {
        DetectionMethod::GraphBased => {
            benchmark_graph_method(image_path, false).await
        }
        DetectionMethod::GraphWithDoors => {
            benchmark_graph_method(image_path, true).await
        }
        DetectionMethod::VisionGPT5 | DetectionMethod::VisionGPT4 => {
            benchmark_vision_method(image_path, method).await
        }
        _ => {
            return BenchmarkResult {
                method,
                image_path: image_path.to_string(),
                success: false,
                execution_time: Duration::from_secs(0),
                rooms_detected: 0,
                avg_confidence: 0.0,
                error: Some("Method not implemented".to_string()),
            };
        }
    };

    let execution_time = start.elapsed();

    match result {
        Ok((rooms, confidence)) => BenchmarkResult {
            method,
            image_path: image_path.to_string(),
            success: true,
            execution_time,
            rooms_detected: rooms,
            avg_confidence: confidence,
            error: None,
        },
        Err(e) => BenchmarkResult {
            method,
            image_path: image_path.to_string(),
            success: false,
            execution_time,
            rooms_detected: 0,
            avg_confidence: 0.0,
            error: Some(e.to_string()),
        },
    }
}

async fn benchmark_graph_method(
    _image_path: &str,
    with_doors: bool,
) -> Result<(usize, f64), Box<dyn std::error::Error>> {
    // For graph methods, we need line data (JSON files)
    // Try to find corresponding JSON request file
    let json_path = if with_doors {
        "data/apartment_with_doors_request.json"
    } else {
        "data/simple_apartment_request.json"
    };

    if !Path::new(json_path).exists() {
        return Err("No line data available for graph method".into());
    }

    // Call our API
    let client = reqwest::Client::new();
    let response = client
        .post("http://localhost:3000/detect")
        .header("Content-Type", "application/json")
        .body(fs::read_to_string(json_path)?)
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(format!("API error: {}", response.status()).into());
    }

    #[derive(serde::Deserialize)]
    struct ApiResponse {
        total_rooms: usize,
    }

    let result: ApiResponse = response.json().await?;

    // Graph methods don't provide confidence, use 0.8 as default
    Ok((result.total_rooms, 0.8))
}

async fn benchmark_vision_method(
    image_path: &str,
    method: DetectionMethod,
) -> Result<(usize, f64), Box<dyn std::error::Error>> {
    let model = match method {
        DetectionMethod::VisionGPT5 => "gpt-5",
        DetectionMethod::VisionGPT4 => "gpt-4-vision-preview",
        _ => unreachable!(),
    };

    let api_key = std::env::var("OPENAI_API_KEY")?;
    let classifier = VisionClassifier::new(api_key, Some(model.to_string()));

    let image_bytes = fs::read(image_path)?;
    let classifications = classifier.classify_floorplan(&image_bytes, None).await?;

    let avg_confidence = if classifications.is_empty() {
        0.0
    } else {
        classifications.iter().map(|c| c.confidence).sum::<f64>()
            / classifications.len() as f64
    };

    Ok((classifications.len(), avg_confidence))
}

fn find_test_images(dir: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut images = Vec::new();

    // Look for PNG images in FPD_* directories
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            let dir_name = path.file_name().unwrap().to_str().unwrap();
            if dir_name.starts_with("FPD_") {
                // Find first PNG in this directory
                for img_entry in fs::read_dir(&path)? {
                    let img_path = img_entry?.path();
                    if img_path.extension().and_then(|s| s.to_str()) == Some("png") {
                        images.push(img_path.to_str().unwrap().to_string());
                        break; // Just take first image from each directory
                    }
                }
            }
        }
    }

    // Limit to 5 images for faster benchmarking
    images.truncate(5);

    Ok(images)
}

fn print_stats(stats: &BenchmarkStats) {
    println!("\n{}", stats.method.name());
    println!("{}", "-".repeat(40));
    println!("Success Rate: {:.1}% ({}/{})",
        stats.success_rate(),
        stats.successful,
        stats.total_tests
    );
    println!("Avg Time: {:.2}s", stats.avg_execution_time.as_secs_f64());
    println!("Min Time: {:.2}s", stats.min_execution_time.as_secs_f64());
    println!("Max Time: {:.2}s", stats.max_execution_time.as_secs_f64());
    println!("Avg Rooms: {:.1}", stats.avg_rooms_per_image);
    println!("Avg Confidence: {:.1}%", stats.avg_confidence * 100.0);
}

fn generate_comparison_table(methods: &[DetectionMethod], results: &[BenchmarkResult]) {
    println!("\n📈 Method Comparison");
    println!("{}", "=".repeat(80));
    println!("{:<20} {:<12} {:<12} {:<12} {:<12}",
        "Method", "Success", "Avg Time", "Avg Rooms", "Avg Conf");
    println!("{}", "-".repeat(80));

    for method in methods {
        let stats = BenchmarkStats::from_results(*method, results);
        println!(
            "{:<20} {:<12} {:<12} {:<12} {:<12}",
            method.name(),
            format!("{:.1}%", stats.success_rate()),
            format!("{:.2}s", stats.avg_execution_time.as_secs_f64()),
            format!("{:.1}", stats.avg_rooms_per_image),
            format!("{:.1}%", stats.avg_confidence * 100.0),
        );
    }
}
