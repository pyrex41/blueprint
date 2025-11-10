use std::env;
use std::path::Path;
use vtracer::{convert_image_to_svg, Config, ColorMode, Hierarchical};
use visioncortex::PathSimplifyMode;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: cargo run --bin vtracer-test <input.png> <output.svg>");
        std::process::exit(1);
    }

    let input_path = Path::new(&args[1]);
    let output_path = Path::new(&args[2]);

    if !input_path.exists() {
        eprintln!("Input file not found: {}", input_path.display());
        std::process::exit(1);
    }

    let config = Config {
        color_mode: ColorMode::Binary,
        hierarchical: Hierarchical::Stacked,
        mode: PathSimplifyMode::Polygon,
        filter_speckle: 4,
        color_precision: 6,
        layer_difference: 16,
        corner_threshold: 60,
        length_threshold: 4.0,
        max_iterations: 10,
        splice_threshold: 45,
        path_precision: Some(3),
    };

    if let Err(e) = convert_image_to_svg(input_path, output_path, config) {
        eprintln!("Error vectorizing: {}", e);
        std::process::exit(1);
    }

    println!("Successfully vectorized {} to {} in polygon mode", input_path.display(), output_path.display());
}
