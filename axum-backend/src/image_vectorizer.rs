use anyhow::{Context, Result};
use visioncortex::PathSimplifyMode;
use vtracer::{convert_image_to_svg, Config, ColorMode, Hierarchical};

#[derive(Debug, Clone)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone)]
pub struct Line {
    pub start: Point,
    pub end: Point,
    pub is_load_bearing: bool,
}

/// Vectorize a raster image and extract line segments using AI parser
pub async fn vectorize_image_ai(image_bytes: &[u8]) -> Result<Vec<Line>> {
    // Load image
    let img = image::load_from_memory(image_bytes)
        .context("Failed to load image from memory")?;

    // Save to temporary file (VTracer requires file path)
    let temp_path = std::env::temp_dir().join(format!("floorplan_{}.png", std::process::id()));
    img.save(&temp_path)
        .context("Failed to save temporary image")?;

    // Save SVG output path
    let svg_path = std::env::temp_dir().join(format!("floorplan_{}.svg", std::process::id()));

    // Configure VTracer for architectural line drawing
    let config = Config {
        color_mode: ColorMode::Binary, // Black and white
        hierarchical: Hierarchical::Stacked,
        mode: PathSimplifyMode::Spline, // Use spline simplification for smooth lines
        filter_speckle: 4, // Remove small artifacts
        color_precision: 6,
        layer_difference: 16,
        corner_threshold: 60, // Prefer straight lines
        length_threshold: 4.0,
        max_iterations: 10,
        splice_threshold: 45,
        path_precision: Some(3),
    };

    // Vectorize image to SVG
    convert_image_to_svg(&temp_path, &svg_path, config)
        .map_err(|e| anyhow::anyhow!("Failed to vectorize image to SVG: {}", e))?;

    // Read SVG output
    let svg_string = std::fs::read_to_string(&svg_path)
        .context("Failed to read generated SVG")?;

    // Clean up temporary files
    let _ = std::fs::remove_file(&temp_path);
    let _ = std::fs::remove_file(&svg_path);

    // Parse SVG using AI
    let lines = ai_parse_svg_to_lines(&svg_string).await?;

    Ok(lines)
}

/// Parse SVG paths and convert to line segments (internal use only)
pub(crate) fn parse_svg_to_lines(svg: &str) -> Result<Vec<Line>> {
    let mut lines = Vec::new();

    // Parse <path> elements
    for path_match in svg.split("<path").skip(1) {
        if let Some(d_attr_start) = path_match.find(" d=\"") {
            let path_data = &path_match[d_attr_start + 4..];
            if let Some(end_quote) = path_data.find('"') {
                let path_commands = &path_data[..end_quote];

                // Parse path commands (simplified - handles M, L, H, V)
                let path_lines = parse_path_commands(path_commands);
                lines.extend(path_lines);
            }
        }
    }

    // Parse <line> elements
    for line_match in svg.split("<line").skip(1) {
        if let Some(end_tag) = line_match.find('>') {
            let line_attrs = &line_match[..end_tag];

            // Extract x1, y1, x2, y2 attributes
            let x1 = extract_attr_value(line_attrs, "x1");
            let y1 = extract_attr_value(line_attrs, "y1");
            let x2 = extract_attr_value(line_attrs, "x2");
            let y2 = extract_attr_value(line_attrs, "y2");

            if let (Some(x1), Some(y1), Some(x2), Some(y2)) = (x1, y1, x2, y2) {
                if let (Ok(x1), Ok(y1), Ok(x2), Ok(y2)) = (x1.parse::<f64>(), y1.parse::<f64>(), x2.parse::<f64>(), y2.parse::<f64>()) {
                    lines.push(Line {
                        start: Point { x: x1, y: y1 },
                        end: Point { x: x2, y: y2 },
                        is_load_bearing: false,
                    });
                }
            }
        }
    }

    // Parse <rect> elements
    for rect_match in svg.split("<rect").skip(1) {
        if let Some(end_tag) = rect_match.find('>') {
            let rect_attrs = &rect_match[..end_tag];

            // Extract x, y, width, height attributes
            let x = extract_attr_value(rect_attrs, "x").unwrap_or("0".to_string());
            let y = extract_attr_value(rect_attrs, "y").unwrap_or("0".to_string());
            let width = extract_attr_value(rect_attrs, "width");
            let height = extract_attr_value(rect_attrs, "height");

            if let (Some(width), Some(height)) = (width, height) {
                if let (Ok(x), Ok(y), Ok(w), Ok(h)) = (x.parse::<f64>(), y.parse::<f64>(), width.parse::<f64>(), height.parse::<f64>()) {
                    // Convert rect to 4 lines
                    let x2 = x + w;
                    let y2 = y + h;

                    lines.extend(vec![
                        Line { start: Point { x, y }, end: Point { x: x2, y }, is_load_bearing: false }, // top
                        Line { start: Point { x: x2, y }, end: Point { x: x2, y: y2 }, is_load_bearing: false }, // right
                        Line { start: Point { x: x2, y: y2 }, end: Point { x, y: y2 }, is_load_bearing: false }, // bottom
                        Line { start: Point { x, y: y2 }, end: Point { x, y }, is_load_bearing: false }, // left
                    ]);
                }
            }
        }
    }

    Ok(lines)
}

/// Extract attribute value from SVG element attributes
fn extract_attr_value(attrs: &str, attr_name: &str) -> Option<String> {
    let attr_pattern = format!("{}=\"", attr_name);
    if let Some(start) = attrs.find(&attr_pattern) {
        let value_start = start + attr_pattern.len();
        if let Some(end) = attrs[value_start..].find('"') {
            return Some(attrs[value_start..value_start + end].to_string());
        }
    }
    None
}

/// Parse SVG path commands into Line segments
fn parse_path_commands(commands: &str) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut current_pos = Point { x: 0.0, y: 0.0 };
    let mut path_start = current_pos.clone();

    // Tokenize the path string
    let tokens: Vec<&str> = commands
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|s| !s.is_empty())
        .collect();

    let mut i = 0;
    while i < tokens.len() {
        let cmd = tokens[i].chars().next().unwrap_or(' ');

        match cmd {
            'M' | 'm' => {
                // Move to
                if i + 2 < tokens.len() {
                    let x = tokens[i + 1].trim_start_matches('M').trim_start_matches('m')
                        .parse::<f64>().unwrap_or(0.0);
                    let y = tokens[i + 2].parse::<f64>().unwrap_or(0.0);

                    if cmd.is_lowercase() {
                        current_pos.x += x;
                        current_pos.y += y;
                    } else {
                        current_pos.x = x;
                        current_pos.y = y;
                    }
                    path_start = current_pos.clone();
                    i += 3;
                } else {
                    i += 1;
                }
            },
            'L' | 'l' => {
                // Line to
                if i + 2 < tokens.len() {
                    let x = tokens[i + 1].trim_start_matches('L').trim_start_matches('l')
                        .parse::<f64>().unwrap_or(0.0);
                    let y = tokens[i + 2].parse::<f64>().unwrap_or(0.0);

                    let start = current_pos.clone();

                    if cmd.is_lowercase() {
                        current_pos.x += x;
                        current_pos.y += y;
                    } else {
                        current_pos.x = x;
                        current_pos.y = y;
                    }

                    lines.push(Line {
                        start,
                        end: current_pos.clone(),
                        is_load_bearing: false, // Default value
                    });
                    i += 3;
                } else {
                    i += 1;
                }
            },
            'H' | 'h' => {
                // Horizontal line
                if i + 1 < tokens.len() {
                    let x = tokens[i + 1].trim_start_matches('H').trim_start_matches('h')
                        .parse::<f64>().unwrap_or(0.0);

                    let start = current_pos.clone();

                    if cmd.is_lowercase() {
                        current_pos.x += x;
                    } else {
                        current_pos.x = x;
                    }

                    lines.push(Line {
                        start,
                        end: current_pos.clone(),
                        is_load_bearing: false,
                    });
                    i += 2;
                } else {
                    i += 1;
                }
            },
            'V' | 'v' => {
                // Vertical line
                if i + 1 < tokens.len() {
                    let y = tokens[i + 1].trim_start_matches('V').trim_start_matches('v')
                        .parse::<f64>().unwrap_or(0.0);

                    let start = current_pos.clone();

                    if cmd.is_lowercase() {
                        current_pos.y += y;
                    } else {
                        current_pos.y = y;
                    }

                    lines.push(Line {
                        start,
                        end: current_pos.clone(),
                        is_load_bearing: false,
                    });
                    i += 2;
                } else {
                    i += 1;
                }
            },
            'Z' | 'z' => {
                // Close path
                if (current_pos.x - path_start.x).abs() > 0.1 ||
                   (current_pos.y - path_start.y).abs() > 0.1 {
                    lines.push(Line {
                        start: current_pos.clone(),
                        end: path_start.clone(),
                        is_load_bearing: false,
                    });
                }
                current_pos = path_start.clone();
                i += 1;
            },
            _ => {
                i += 1;
            }
        }
    }

    // Filter out very short lines (noise)
    lines.into_iter()
        .filter(|line| {
            let dx = line.end.x - line.start.x;
            let dy = line.end.y - line.start.y;
            let length = (dx * dx + dy * dy).sqrt();
            length > 5.0 // Minimum line length threshold
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::test;

    #[tokio::test]
    async fn test_parse_simple_path() {
        let svg_path = "M 0,0 L 100,0 L 100,100 L 0,100 Z";
        let lines = parse_path_commands(svg_path);

        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0].start.x, 0.0);
        assert_eq!(lines[0].end.x, 100.0);
    }

    #[tokio::test]
    async fn test_parse_svg_to_lines() {
        let svg = r#"<svg viewBox="0 0 400 300" xmlns="http://www.w3.org/2000/svg">
            <rect x="50" y="50" width="300" height="200" fill="none" stroke="black" stroke-width="2"/>
            <line x1="150" y1="50" x2="150" y2="250" stroke="black" stroke-width="2"/>
            <line x1="250" y1="50" x2="250" y2="250" stroke="black" stroke-width="2"/>
            <line x1="50" y1="150" x2="350" y2="150" stroke="black" stroke-width="2"/>
        </svg>"#;

        let lines = parse_svg_to_lines(svg).unwrap();
        println!("Parsed {} lines from SVG", lines.len());
        assert!(!lines.is_empty(), "Should parse at least some lines");

        // Check that we have reasonable coordinates
        for line in &lines {
            assert!(line.start.x >= 0.0 && line.start.x <= 400.0);
            assert!(line.start.y >= 0.0 && line.start.y <= 300.0);
            assert!(line.end.x >= 0.0 && line.end.x <= 400.0);
            assert!(line.end.y >= 0.0 && line.end.y <= 300.0);
        }
    }

#[tokio::test]
async fn test_ai_parse_svg_to_lines() {
    // Requires OPENAI_API_KEY env var set
    let svg_content = r#"<svg viewBox="0 0 100 100"><path d="M 0,0 L 100,0 L 100,100 L 0,100 Z" fill="none" stroke="black"/></svg>"#;
    
    match ai_parse_svg_to_lines(svg_content).await {
        Ok(lines) => {
            assert!(!lines.is_empty(), "Should extract at least one line");
            assert!(lines.len() <= 4, "Should not extract more than 4 lines for simple square");
            println!("AI parsed {} lines successfully", lines.len());
        }
        Err(e) => {
            eprintln!("Test failed (likely missing API key): {}", e);
            // Don't panic if API key missing, just warn
            panic!("AI parse failed: {}", e);
        }
    }
}

#[tokio::test]
async fn test_vectorize_image_ai() {
    // Create a simple test image: a black square on white background (simple room)
    let mut img = image::RgbaImage::new(100, 100);
    // Draw a square outline
    for x in 0..100 {
        img.put_pixel(x, 0, image::Rgba([0, 0, 0, 255])); // top
        img.put_pixel(x, 99, image::Rgba([0, 0, 0, 255])); // bottom
    }
    for y in 0..100 {
        img.put_pixel(0, y, image::Rgba([0, 0, 0, 255])); // left
        img.put_pixel(99, y, image::Rgba([0, 0, 0, 255])); // right
    }

    let image_bytes = img.as_raw().to_vec(); // Note: This is raw RGBA, need to save as PNG for proper bytes

    // Actually save to bytes
    let mut png_buffer = Vec::new();
    {
        let mut writer = std::io::Cursor::new(&mut png_buffer);
        img.write_to(&mut writer, image::ImageFormat::Png).unwrap();
    }
    let image_bytes = &png_buffer;

    match vectorize_image_ai(image_bytes).await {
        Ok(lines) => {
            println!("AI vectorization extracted {} lines from test image", lines.len());
            for line in &lines {
                println!("Line: ({:.1}, {:.1}) to ({:.1}, {:.1}), load-bearing: {}", 
                    line.start.x, line.start.y, line.end.x, line.end.y, line.is_load_bearing);
            }
            assert!(!lines.is_empty(), "Should extract lines from square image");
        }
        Err(e) => {
            eprintln!("AI vectorization failed (likely missing API key or vtracer issue): {}", e);
            panic!("Vectorization failed: {}", e);
        }
    }
}
}

use reqwest;
use serde_json::Value;
use std::error::Error;

pub async fn ai_parse_svg_to_lines(svg: &str) -> Result<Vec<Line>> {
    let api_key = std::env::var("OPENAI_API_KEY").map_err(|_| anyhow::anyhow!("OPENAI_API_KEY not set"))?;

    let client = reqwest::Client::new();

    let system_prompt = r#"You are an architectural SVG parser. Parse the SVG to extract wall and door segments.

For walls: long straight lines, is_load_bearing: true

For doors: short lines or gaps, is_load_bearing: false, interpolate if needed by connecting endpoints.

Output ONLY JSON: { "walls": [ {"start": {"x": f64, "y": f64}, "end": {"x": f64, "y": f64}, "is_load_bearing": bool } ] }

Use SVG coordinate system. Filter lines <5 units."#;

    let user_prompt = format!("SVG content:\n{svg}");

    let request_body = serde_json::json!({
        "model": "gpt-5-nano",
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_prompt}
        ],
        "response_format": {"type": "json_object"},
        "max_completion_tokens": 4096
    });

    let response = client
        .post("https://api.openai.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&request_body)
        .send()
        .await?;

    let api_response: Value = response.json().await?;

    let content = api_response["choices"][0]["message"]["content"]
        .as_str()
        .ok_or(anyhow::anyhow!("No content in response"))?;

    let parsed: Value = serde_json::from_str(content)?;

    let walls_array = parsed["walls"]
        .as_array()
        .ok_or(anyhow::anyhow!("No walls array"))?
        .to_vec();

    let mut lines = Vec::new();

    for wall_val in walls_array {
        let start_x = wall_val["start"]["x"].as_f64().unwrap_or(0.0);
        let start_y = wall_val["start"]["y"].as_f64().unwrap_or(0.0);
        let end_x = wall_val["end"]["x"].as_f64().unwrap_or(0.0);
        let end_y = wall_val["end"]["y"].as_f64().unwrap_or(0.0);
        let is_load = wall_val["is_load_bearing"].as_bool().unwrap_or(false);

        let line = Line {
            start: Point { x: start_x, y: start_y },
            end: Point { x: end_x, y: end_y },
            is_load_bearing: is_load,
        };

        let length = ((end_x - start_x).powi(2) + (end_y - start_y).powi(2)).sqrt();
        if length > 5.0 {
            lines.push(line);
        }
    }

    Ok(lines)
}
