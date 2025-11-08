// Example: Classify a floorplan image using GPT-5 Vision
use std::fs;
use vision_classifier::VisionClassifier;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    println!("🔍 GPT-5 Vision Floorplan Classifier");
    println!("=====================================\n");

    // Check for OPENAI_API_KEY
    if std::env::var("OPENAI_API_KEY").is_err() {
        eprintln!("❌ Error: OPENAI_API_KEY environment variable not set");
        eprintln!("\nPlease set your OpenAI API key:");
        eprintln!("  export OPENAI_API_KEY=sk-...");
        return Ok(());
    }

    // Get image path from command line or use default
    let args: Vec<String> = std::env::args().collect();
    let image_path = args.get(1).map(|s| s.as_str()).unwrap_or_else(|| {
        println!("ℹ️  No image path provided, using sample from data/");
        "data/FPD_2_FULL_COMPACTNESS/FPD_247_1737914641778_2_FULL_COMPACTNESS.png"
    });

    println!("📂 Loading image: {}", image_path);

    // Load image
    let image_bytes = match fs::read(image_path) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("❌ Failed to read image: {}", e);
            eprintln!("\nUsage: cargo run --example classify_image [IMAGE_PATH]");
            eprintln!("Example: cargo run --example classify_image data/FPD_2_FULL_COMPACTNESS/FPD_247_1737914641778_2_FULL_COMPACTNESS.png");
            return Ok(());
        }
    };

    println!("✅ Loaded {} bytes\n", image_bytes.len());

    // Create classifier
    let classifier = VisionClassifier::from_env()?;

    println!("🤖 Analyzing floorplan with GPT-5...");
    println!("(This may take 5-10 seconds)\n");

    // Classify
    match classifier.classify_floorplan(&image_bytes, None).await {
        Ok(classifications) => {
            println!("✅ Classification complete!\n");
            println!("📊 Results:");
            println!("{}", "=".repeat(70));

            for (i, room) in classifications.iter().enumerate() {
                println!("\n🏠 Room #{}: {}", i + 1, room.room_type.to_uppercase());
                println!("   Confidence: {:.1}%", room.confidence * 100.0);
                println!("   Features: {}", room.features.join(", "));
                println!("   Description: {}", room.description);
            }

            println!("\n{}", "=".repeat(70));
            println!("\n📈 Summary: Detected {} rooms", classifications.len());

            // Save results to JSON
            let output_path = "data/gpt5_classification_result.json";
            let json = serde_json::to_string_pretty(&classifications)?;
            fs::write(output_path, json)?;
            println!("💾 Saved results to: {}", output_path);
        }
        Err(e) => {
            eprintln!("❌ Classification failed: {}", e);
            eprintln!("\nPossible issues:");
            eprintln!("  - Invalid OPENAI_API_KEY");
            eprintln!("  - Network connection problem");
            eprintln!("  - API rate limit exceeded");
            eprintln!("  - Model 'gpt-5' not available (try 'gpt-4-vision-preview')");
        }
    }

    Ok(())
}
