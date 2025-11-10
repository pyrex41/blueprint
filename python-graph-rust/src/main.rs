use anyhow::{Context, Result};
use image::GrayImage;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::str::FromStr;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Room {
    pub id: usize,
    pub bounding_box: [f64; 4],
    pub area: f64,
    pub name_hint: String,
    pub points: Vec<Point>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LineSegment {
    pub start: Point,
    pub end: Point,
}

fn parse_svg_paths(svg_content: &str) -> Result<Vec<LineSegment>, Box<dyn std::error::Error>> {
    // Simple SVG path parser - approximate Bezier curves with line segments
    let segments = Vec::new();
    let root = svg::parser::parse(svg_content)?;
    
    // Parse path elements
    for node in root.descendants() {
        if let Some(path) = node.as_path() {
            let path_data = path.data();
            // Parse path d attribute (simplified - handles M, L, Z commands)
            let d = path_data.iter().map(|d| d.to_string()).collect::<String>();
            let tokens = d.split_whitespace().collect::<Vec<&str>>();
            
            let mut i = 0;
            let mut current_pos = Point { x: 0.0, y: 0.0 };
            let mut start_pos = Point { x: 0.0, y: 0.0 };
            
            while i < tokens.len() {
                let token = tokens[i];
                i += 1;
                
                match token {
                    "M" | "m" => {
                        if i + 1 < tokens.len() {
                            current_pos.x = tokens[i].parse()?;
                            current_pos.y = tokens[i+1].parse()?;
                            start_pos = current_pos;
                            i += 2;
                        }
                    }
                    "L" | "l" => {
                        while i + 1 < tokens.len() {
                            let x = tokens[i].parse::<f64>()?;
                            let y = tokens[i+1].parse::<f64>()?;
                            segments.push(LineSegment {
                                start: current_pos.clone(),
                                end: Point { x: current_pos.x + x, y: current_pos.y + y },
                            });
                            current_pos = Point { x: current_pos.x + x, y: current_pos.y + y };
                            i += 2;
                        }
                    }
                    "Z" | "z" => {
                        if current_pos != start_pos {
                            segments.push(LineSegment {
                                start: current_pos.clone(),
                                end: start_pos.clone(),
                            });
                        }
                        current_pos = start_pos;
                    }
                    _ => {
                        // Skip other commands for now
                        if let Ok(num) = token.parse::<f64>() {
                            // Handle numeric tokens as coordinates
                            if i < tokens.len() {
                                let y = tokens[i].parse::<f64>()?;
                                segments.push(LineSegment {
                                    start: current_pos.clone(),
                                    end: Point { x: num, y },
                                });
                                current_pos = Point { x: num, y };
                                i += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    
    Ok(segments)
}

fn find_intersections(segments: &[LineSegment]) -> Vec<Point> {
    let mut vertices = Vec::new();
    
    // Add all endpoints
    for seg in segments {
        vertices.push(seg.start.clone());
        vertices.push(seg.end.clone());
    }
    
    // Find intersections between segments
    for i in 0..segments.len() {
        for j in (i + 1)..segments.len() {
            if let Some(inter) = line_intersection(&segments[i], &segments[j]) {
                vertices.push(inter);
            }
        }
    }
    
    // Remove duplicates (simple epsilon comparison)
    let mut unique_vertices: Vec<Point> = Vec::new();
    for v in vertices {
        let mut is_duplicate = false;
        for existing in &unique_vertices {
            if (v.x - existing.x).abs() < 1e-6 && (v.y - existing.y).abs() < 1e-6 {
                is_duplicate = true;
                break;
            }
        }
        if !is_duplicate {
            unique_vertices.push(v);
        }
    }
    
    unique_vertices
}

fn line_intersection(seg1: &LineSegment, seg2: &LineSegment) -> Option<Point> {
    let (x1, y1) = (seg1.start.x, seg1.start.y);
    let (x2, y2) = (seg1.end.x, seg1.end.y);
    let (x3, y3) = (seg2.start.x, seg2.start.y);
    let (x4, y4) = (seg2.end.x, seg2.end.y);
    
    let denom = (y4 - y3) * (x2 - x1) - (x4 - x3) * (y2 - y1);
    if denom.abs() < 1e-10 {
        return None;
    }
    
    let t = ((x1 - x3) * (y4 - y3) - (y1 - y3) * (x4 - x3)) / denom;
    let u = -((y4 - y3) * (x1 - x3) - (x4 - x3) * (y1 - y3)) / denom;
    
    if t >= 0.0 && t <= 1.0 && u >= 0.0 && u <= 1.0 {
        Some(Point {
            x: x1 + t * (x2 - x1),
            y: y1 + t * (y2 - y1),
        })
    } else {
        None
    }
}

fn build_graph(vertices: &[Point], segments: &[LineSegment]) -> HashMap<usize, Vec<usize>> {
    let mut point_to_idx: HashMap<(f64, f64), usize> = HashMap::new();
    for (i, v) in vertices.iter().enumerate() {
        point_to_idx.insert((v.x, v.y), i);
    }
    
    let mut graph: HashMap<usize, Vec<usize>> = HashMap::new();
    
    for seg in segments {
        if let (Some(i1), Some(i2)) = (point_to_idx.get(&(seg.start.x, seg.start.y)), point_to_idx.get(&(seg.end.x, seg.end.y))) {
            graph.entry(*i1).or_insert_with(Vec::new).push(*i2);
            graph.entry(*i2).or_insert_with(Vec::new).push(*i1);
        }
    }
    
    graph
}

fn find_cycles(graph: &HashMap<usize, Vec<usize>>, start: usize) -> Vec<Vec<usize>> {
    let mut cycles = Vec::new();
    let mut stack = vec![(start, vec![start])];
    
    while let Some((node, path)) = stack.pop() {
        for &neighbor in graph.get(&node).unwrap_or(&Vec::new()) {
            if neighbor == start && path.len() > 2 {
                cycles.push(path.clone());
            } else if !path.contains(&neighbor) {
                let mut new_path = path.clone();
                new_path.push(neighbor);
                stack.push((neighbor, new_path));
            }
        }
    }
    
    cycles
}

fn all_cycles(graph: &HashMap<usize, Vec<usize>>) -> Vec<Vec<usize>> {
    let mut all_cycles = Vec::new();
    let mut visited_starts = std::collections::HashSet::new();
    
    for &start in graph.keys() {
        if !visited_starts.contains(&start) {
            let cycles = find_cycles(graph, start);
            for cycle in cycles {
                // Deduplicate by sorting and using set
                let mut sorted_cycle = cycle.clone();
                sorted_cycle.sort();
                if !all_cycles.iter().any(|c| c == &sorted_cycle) {
                    all_cycles.push(cycle);
                }
            }
            visited_starts.insert(start);
        }
    }
    
    all_cycles
}

fn point_in_polygon(point: &Point, poly: &[Point]) -> bool {
    let (x, y) = (point.x, point.y);
    let n = poly.len();
    let mut inside = false;
    let mut p1x = poly[0].x;
    let mut p1y = poly[0].y;
    
    for i in 0..n {
        let p2x = poly[(i + 1) % n].x;
        let p2y = poly[(i + 1) % n].y;
        
        if y > p1y.min(p2y) {
            if y <= p1y.max(p2y) {
                if x <= p1x.max(p2x) {
                    if p1y != p2y {
                        let xinters = (y - p1y) * (p2x - p1x) / (p2y - p1y) + p1x;
                        if p1x == p2x || x <= xinters {
                            inside = !inside;
                        }
                    }
                }
            }
        }
        p1x = p2x;
        p1y = p2y;
    }
    
    inside
}

fn is_minimal_cycle(cycle: &[usize], vertices: &[Point], graph: &HashMap<usize, Vec<usize>>) -> bool {
    if cycle.len() < 3 {
        return false;
    }
    
    let poly_points: Vec<Point> = cycle.iter().map(|&i| vertices[i].clone()).collect();
    
    // Check if any other vertices are inside this polygon
    for (i, v) in vertices.iter().enumerate() {
        if !cycle.contains(&i) {
            if point_in_polygon(v, &poly_points) {
                return false;
            }
        }
    }
    
    true
}

fn compute_bounding_box(points: &[Point]) -> [f64; 4] {
    if points.is_empty() {
        return [0.0, 0.0, 0.0, 0.0];
    }
    
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    
    for p in points {
        min_x = min_x.min(p.x);
        min_y = min_y.min(p.y);
        max_x = max_x.max(p.x);
        max_y = max_y.max(p.y);
    }
    
    [min_x, min_y, max_x, max_y]
}

fn compute_area(points: &[Point]) -> f64 {
    if points.len() < 3 {
        return 0.0;
    }
    
    let mut area = 0.0;
    let n = points.len();
    
    for i in 0..n {
        let j = (i + 1) % n;
        area += points[i].x * points[j].y;
        area -= points[j].x * points[i].y;
    }
    
    area.abs() / 2.0
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let image_path = if args.len() > 1 {
        args[1].clone()
    } else {
        "test-data/test_blueprint_004.png".to_string()
    };
    
    println!("Python Graph Detection in Rust");
    println!("Loading image: {}", image_path);
    
    // Step 1: Convert PNG to SVG using VTracer
    let temp_svg = std::env::temp_dir().join("temp_input.svg");
    let vtracer_cmd = Command::new("vtracer")
        .arg("--binary")
        .arg("--output")
        .arg(temp_svg.to_str().unwrap())
        .arg(&image_path)
        .output()?;
    
    if !vtracer_cmd.status.success() {
        return Err(anyhow::anyhow!("VTracer failed: {}", String::from_utf8_lossy(&vtracer_cmd.stderr)));
    }
    
    // Step 2: Read SVG content
    let svg_content = fs::read_to_string(&temp_svg)?;
    fs::remove_file(&temp_svg)?;
    
    println!("SVG generated, parsing paths...");
    
    // Step 3: Parse SVG paths to line segments
    let segments = parse_svg_paths(&svg_content)?;
    println!("Parsed {} line segments", segments.len());
    
    // Step 4: Find vertices (endpoints + intersections)
    let vertices = find_intersections(&segments);
    println!("Found {} vertices", vertices.len());
    
    // Step 5: Build graph
    let graph = build_graph(&vertices, &segments);
    println!("Graph built with {} nodes", graph.len());
    
    // Step 6: Find cycles
    let cycles = all_cycles(&graph);
    println!("Found {} cycles", cycles.len());
    
    // Step 7: Filter minimal cycles (rooms)
    let mut minimal_cycles = Vec::new();
    for cycle in cycles {
        if is_minimal_cycle(&cycle, &vertices, &graph) {
            minimal_cycles.push(cycle);
        }
    }
    println!("Found {} minimal cycles (potential rooms)", minimal_cycles.len());
    
    // Step 8: Generate rooms
    let mut rooms = Vec::new();
    for (i, cycle) in minimal_cycles.iter().enumerate() {
        let points: Vec<Point> = cycle.iter().map(|&idx| vertices[idx].clone()).collect();
        let bbox = compute_bounding_box(&points);
        let area = compute_area(&points);
        
        if area > 100.0 {  // Filter small cycles
            rooms.push(Room {
                id: i,
                bounding_box: bbox,
                area,
                name_hint: "Room".to_string(),
                points,
            });
        }
    }
    
    // Step 9: Output JSON
    let output = json!({
        "rooms": rooms,
        "total_rooms": rooms.len(),
        "vertices": vertices.len(),
        "segments": segments.len(),
        "cycles_found": cycles.len(),
        "minimal_cycles": minimal_cycles.len()
    });
    
    let json_str = serde_json::to_string_pretty(&output)?;
    fs::write("detected_rooms_python_graph.json", json_str)?;
    
    println!("Detected {} rooms", rooms.len());
    println!("Output saved to detected_rooms_python_graph.json");
    
    Ok(())
}