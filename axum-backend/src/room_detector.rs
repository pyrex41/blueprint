use crate::{graph_builder::FloorplanGraph, Point, Room};
use geo::{Area, Coord, LineString, Polygon as GeoPolygon};
use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;
use std::collections::{HashMap, HashSet, VecDeque};

/// Detect rooms in a floorplan graph by finding cycles
pub fn detect_rooms(graph: &FloorplanGraph, area_threshold: f64) -> Vec<Room> {
    let cycles = find_all_cycles(graph);
    let mut rooms = Vec::new();

    for (id, cycle_nodes) in cycles.iter().enumerate() {
        // Extract points from cycle
        let points: Vec<Point> = cycle_nodes
            .iter()
            .map(|&node_idx| graph[node_idx].clone())
            .collect();

        if points.len() < 3 {
            continue; // Not a valid polygon
        }

        // Calculate area
        let area = calculate_polygon_area(&points);

        // Filter by area threshold
        if area < area_threshold {
            continue;
        }

        // Calculate bounding box
        let bbox = calculate_bounding_box(&points);

        // Generate heuristic name
        let name_hint = generate_room_name(area, &bbox);

        rooms.push(Room {
            id,
            bounding_box: bbox,
            area,
            name_hint,
            points,
        });
    }

    rooms
}

/// Find all simple cycles in the graph using DFS
fn find_all_cycles(graph: &FloorplanGraph) -> Vec<Vec<NodeIndex>> {
    let mut cycles = Vec::new();
    let mut visited_global = HashSet::new();

    for node in graph.node_indices() {
        if visited_global.contains(&node) {
            continue;
        }

        let mut path = Vec::new();
        let mut visited_local = HashSet::new();

        dfs_find_cycles(
            graph,
            node,
            None,
            &mut path,
            &mut visited_local,
            &mut visited_global,
            &mut cycles,
        );
    }

    // Remove duplicate cycles (same cycle starting from different nodes)
    deduplicate_cycles(cycles)
}

fn dfs_find_cycles(
    graph: &FloorplanGraph,
    current: NodeIndex,
    parent: Option<NodeIndex>,
    path: &mut Vec<NodeIndex>,
    visited_local: &mut HashSet<NodeIndex>,
    visited_global: &mut HashSet<NodeIndex>,
    cycles: &mut Vec<Vec<NodeIndex>>,
) {
    if visited_local.contains(&current) {
        // Found a cycle
        if let Some(cycle_start_pos) = path.iter().position(|&n| n == current) {
            let cycle = path[cycle_start_pos..].to_vec();
            if cycle.len() >= 3 {
                cycles.push(cycle);
            }
        }
        return;
    }

    visited_local.insert(current);
    visited_global.insert(current);
    path.push(current);

    // Explore neighbors
    for edge in graph.edges(current) {
        let neighbor = edge.target();

        // Don't go back to parent immediately (avoid trivial back-and-forth)
        if Some(neighbor) == parent {
            continue;
        }

        dfs_find_cycles(
            graph,
            neighbor,
            Some(current),
            path,
            visited_local,
            visited_global,
            cycles,
        );
    }

    path.pop();
    visited_local.remove(&current);
}

/// Deduplicate cycles that represent the same room
fn deduplicate_cycles(cycles: Vec<Vec<NodeIndex>>) -> Vec<Vec<NodeIndex>> {
    let mut unique_cycles = Vec::new();
    let mut seen_signatures = HashSet::new();

    for cycle in cycles {
        let signature = cycle_signature(&cycle);

        if !seen_signatures.contains(&signature) {
            seen_signatures.insert(signature);
            unique_cycles.push(cycle);
        }
    }

    unique_cycles
}

/// Create a signature for a cycle to identify duplicates
/// Sorts the nodes to handle different starting points
fn cycle_signature(cycle: &[NodeIndex]) -> Vec<u32> {
    let mut sorted = cycle.iter().map(|n| n.index() as u32).collect::<Vec<_>>();
    sorted.sort_unstable();
    sorted
}

/// Calculate the area of a polygon using the Shoelace formula
fn calculate_polygon_area(points: &[Point]) -> f64 {
    if points.len() < 3 {
        return 0.0;
    }

    // Convert to geo::Polygon for area calculation
    let coords: Vec<Coord> = points
        .iter()
        .map(|p| Coord { x: p.x, y: p.y })
        .collect();

    let line_string = LineString::from(coords);
    let polygon = GeoPolygon::new(line_string, vec![]);

    polygon.unsigned_area()
}

/// Calculate the axis-aligned bounding box for a set of points
fn calculate_bounding_box(points: &[Point]) -> [f64; 4] {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for point in points {
        min_x = min_x.min(point.x);
        min_y = min_y.min(point.y);
        max_x = max_x.max(point.x);
        max_y = max_y.max(point.y);
    }

    [min_x, min_y, max_x, max_y]
}

/// Generate a heuristic name for a room based on its properties
fn generate_room_name(area: f64, bbox: &[f64; 4]) -> String {
    let width = bbox[2] - bbox[0];
    let height = bbox[3] - bbox[1];
    let aspect_ratio = width / height;

    // Simple heuristics based on size
    if area < 500.0 {
        "Small Room".to_string()
    } else if area < 2000.0 {
        if aspect_ratio > 1.5 || aspect_ratio < 0.67 {
            "Corridor".to_string()
        } else {
            "Bedroom".to_string()
        }
    } else if area < 5000.0 {
        "Living Room".to_string()
    } else {
        "Large Room".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{graph_builder::build_graph, Line};

    #[test]
    fn test_bounding_box_calculation() {
        let points = vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 10.0, y: 0.0 },
            Point { x: 10.0, y: 5.0 },
            Point { x: 0.0, y: 5.0 },
        ];

        let bbox = calculate_bounding_box(&points);

        assert_eq!(bbox, [0.0, 0.0, 10.0, 5.0]);
    }

    #[test]
    fn test_polygon_area_calculation() {
        // Square with side length 10
        let points = vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 10.0, y: 0.0 },
            Point { x: 10.0, y: 10.0 },
            Point { x: 0.0, y: 10.0 },
        ];

        let area = calculate_polygon_area(&points);

        assert!((area - 100.0).abs() < 1e-6);
    }

    #[test]
    fn test_room_detection_simple_square() {
        // Create a simple square
        let lines = vec![
            Line {
                start: Point { x: 0.0, y: 0.0 },
                end: Point { x: 100.0, y: 0.0 },
                is_load_bearing: false,
            },
            Line {
                start: Point { x: 100.0, y: 0.0 },
                end: Point { x: 100.0, y: 100.0 },
                is_load_bearing: false,
            },
            Line {
                start: Point { x: 100.0, y: 100.0 },
                end: Point { x: 0.0, y: 100.0 },
                is_load_bearing: false,
            },
            Line {
                start: Point { x: 0.0, y: 100.0 },
                end: Point { x: 0.0, y: 0.0 },
                is_load_bearing: false,
            },
        ];

        let graph = build_graph(&lines);
        let rooms = detect_rooms(&graph, 100.0);

        assert!(rooms.len() > 0, "Should detect at least one room");

        if let Some(room) = rooms.first() {
            assert!((room.area - 10000.0).abs() < 100.0, "Area should be close to 10000");
        }
    }

    #[test]
    fn test_room_name_generation() {
        let small_area = 300.0;
        let small_bbox = [0.0, 0.0, 10.0, 30.0];
        assert_eq!(generate_room_name(small_area, &small_bbox), "Small Room");

        let medium_area = 1000.0;
        let square_bbox = [0.0, 0.0, 30.0, 30.0];
        assert_eq!(generate_room_name(medium_area, &square_bbox), "Bedroom");

        let corridor_area = 800.0;
        let corridor_bbox = [0.0, 0.0, 50.0, 10.0];
        assert_eq!(generate_room_name(corridor_area, &corridor_bbox), "Corridor");
    }
}
