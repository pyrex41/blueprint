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

/// Vectorize a raster image and extract line segments
pub fn vectorize_image(image_bytes: &[u8]) -> Result<Vec<Line>> {
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

    // Parse SVG and extract line segments
    let lines = parse_svg_to_lines(&svg_string)?;

    Ok(lines)
}

/// Parse SVG paths and convert to line segments
pub fn parse_svg_to_lines(svg: &str) -> Result<Vec<Line>> {
    let mut lines = Vec::new();

    // Simple SVG path parser - looks for <path d="..."/> elements
    // and extracts M (moveto) and L (lineto) commands

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

    Ok(lines)
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

    #[test]
    fn test_parse_simple_path() {
        let svg_path = "M 0,0 L 100,0 L 100,100 L 0,100 Z";
        let lines = parse_path_commands(svg_path);

        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0].start.x, 0.0);
        assert_eq!(lines[0].end.x, 100.0);
    }
}
