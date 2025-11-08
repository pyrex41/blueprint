// Ensemble detector - combines multiple methods for best results
use instant::Instant;
use serde_json::json;
use std::fs;
use std::time::Duration;
use unified_detector::*;
use vision_classifier::VisionClassifier;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    println!("🎯 Floorplan Detection Ensemble");
    println!("=================================\n");

    // Get image path from args
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: cargo run --bin ensemble <IMAGE_PATH> [--strategy <strategy>]");
        eprintln!("\nStrategies:");
        eprintln!("  fastest           - Use fastest method");
        eprintln!("  confidence        - Use highest confidence");
        eprintln!("  all (default)     - Run all and compare");
        eprintln!("\nExample:");
        eprintln!("  cargo run --bin ensemble data/FPD_2_FULL_COMPACTNESS/FPD_247.png");
        return Ok(());
    }

    let image_path = &args[1];
    let strategy = args.get(3).map(|s| s.as_str()).unwrap_or("all");

    println!("📂 Image: {}", image_path);
    println!("🎲 Strategy: {}\n", strategy);

    // Check which methods are available
    let mut methods = vec![
        DetectionMethod::GraphBased,
        DetectionMethod::GraphWithDoors,
    ];

    if std::env::var("OPENAI_API_KEY").is_ok() {
        methods.push(DetectionMethod::VisionGPT5);
        println!("✅ GPT-5 Vision available");
    } else {
        println!("⚠️  GPT-5 Vision not available (no OPENAI_API_KEY)");
    }

    println!("\n🔬 Running {} detection methods...\n", methods.len());

    // Run all methods
    let mut results = Vec::new();

    for (i, method) in methods.iter().enumerate() {
        println!("▶ {}/{}: {}", i + 1, methods.len(), method.name());

        let start = Instant::now();
        let detection = run_method(*method, image_path).await;
        let elapsed = start.elapsed();

        match detection {
            Ok((rooms, confidence)) => {
                println!("  ✅ {} rooms detected in {:.2}s (conf: {:.1}%)",
                    rooms.len(), elapsed.as_secs_f64(), confidence * 100.0);

                results.push(DetectionResult {
                    method: *method,
                    rooms: rooms.clone(),
                    execution_time: elapsed,
                    metadata: DetectionMetadata {
                        total_rooms: rooms.len(),
                        avg_confidence: confidence,
                        method_specific: json!({}),
                    },
                });
            }
            Err(e) => {
                println!("  ❌ Failed: {}", e);
            }
        }
    }

    println!("\n📊 Results Summary");
    println!("{}", "=".repeat(70));

    // Generate comparison report
    let report = ComparisonReport::new(image_path.to_string(), results);

    println!("\n🏆 Winner: {} (score: {:.2})",
        report.winner.name(),
        report.ranking.first().map(|(_, s)| s).unwrap_or(&0.0)
    );

    println!("\n📈 Ranking:");
    for (i, (method, score)) in report.ranking.iter().enumerate() {
        println!("  {}. {} - score: {:.3}", i + 1, method.name(), score);
    }

    println!("\n📋 Detailed Results:");
    for result in &report.results {
        println!("\n{}", result.method.name());
        println!("{}", "-".repeat(40));
        println!("Rooms: {}", result.metadata.total_rooms);
        println!("Confidence: {:.1}%", result.metadata.avg_confidence * 100.0);
        println!("Time: {:.3}s", result.execution_time.as_secs_f64());

        if !result.rooms.is_empty() {
            println!("Room types:");
            for room in &result.rooms {
                println!("  - {} (area: {:.0}, conf: {:.1}%)",
                    room.room_type, room.area, room.confidence * 100.0);
            }
        }
    }

    // Save report
    let output_path = "data/ensemble_report.json";
    fs::write(output_path, serde_json::to_string_pretty(&report)?)?;
    println!("\n💾 Saved detailed report to: {}", output_path);

    // Print recommendation
    println!("\n💡 Recommendation:");
    match report.winner {
        DetectionMethod::GraphBased | DetectionMethod::GraphWithDoors => {
            println!("   Use geometric method for fast, offline detection");
        }
        DetectionMethod::VisionGPT5 | DetectionMethod::VisionGPT4 => {
            println!("   Use vision LLM for accurate classification");
            println!("   (Trade-off: slower and costs API fees)");
        }
        _ => {}
    }

    Ok(())
}

async fn run_method(
    method: DetectionMethod,
    image_path: &str,
) -> Result<(Vec<Room>, f64), Box<dyn std::error::Error>> {
    match method {
        DetectionMethod::GraphBased => run_graph_method(false).await,
        DetectionMethod::GraphWithDoors => run_graph_method(true).await,
        DetectionMethod::VisionGPT5 | DetectionMethod::VisionGPT4 => {
            run_vision_method(image_path, method).await
        }
        _ => Err("Method not implemented".into()),
    }
}

async fn run_graph_method(
    with_doors: bool,
) -> Result<(Vec<Room>, f64), Box<dyn std::error::Error>> {
    let json_path = if with_doors {
        "data/apartment_with_doors_request.json"
    } else {
        "data/simple_apartment_request.json"
    };

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
    struct ApiRoom {
        id: usize,
        area: f64,
        name_hint: String,
        bounding_box: [f64; 4],
    }

    #[derive(serde::Deserialize)]
    struct ApiResponse {
        rooms: Vec<ApiRoom>,
    }

    let result: ApiResponse = response.json().await?;

    let rooms: Vec<Room> = result
        .rooms
        .into_iter()
        .map(|r| Room {
            id: r.id,
            room_type: r.name_hint,
            confidence: 0.8, // Graph methods don't provide confidence
            area: r.area,
            bounding_box: r.bounding_box,
            features: vec![],
        })
        .collect();

    Ok((rooms, 0.8))
}

async fn run_vision_method(
    image_path: &str,
    method: DetectionMethod,
) -> Result<(Vec<Room>, f64), Box<dyn std::error::Error>> {
    let model = match method {
        DetectionMethod::VisionGPT5 => "gpt-5",
        DetectionMethod::VisionGPT4 => "gpt-4-vision-preview",
        _ => unreachable!(),
    };

    let api_key = std::env::var("OPENAI_API_KEY")?;
    let classifier = VisionClassifier::new(api_key, Some(model.to_string()));

    let image_bytes = fs::read(image_path)?;
    let classifications = classifier.classify_floorplan(&image_bytes, None).await?;

    let rooms: Vec<Room> = classifications
        .iter()
        .map(|c| Room {
            id: c.room_id,
            room_type: c.room_type.clone(),
            confidence: c.confidence,
            area: 0.0, // Vision methods don't provide area
            bounding_box: [0.0, 0.0, 0.0, 0.0],
            features: c.features.clone(),
        })
        .collect();

    let avg_confidence = if rooms.is_empty() {
        0.0
    } else {
        rooms.iter().map(|r| r.confidence).sum::<f64>() / rooms.len() as f64
    };

    Ok((rooms, avg_confidence))
}
