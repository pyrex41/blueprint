use crate::{graph_builder::FloorplanGraph, Point, Room};
use geo::{Area, Coord, LineString, Polygon as GeoPolygon};
use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;
use std::collections::{HashMap, HashSet};
use tracing::{warn, debug};

// Maximum number of cycles to detect (prevent DoS)
const MAX_CYCLES: usize = 1000;
// Maximum cycle length to consider (prevent exponential blowup)
const MAX_CYCLE_LENGTH: usize = 100;

/// Detect rooms in a floorplan graph by finding cycles
pub fn detect_rooms(graph: &FloorplanGraph, area_threshold: f64) -> Vec<Room> {
    let cycles = find_room_cycles(graph);
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

/// Find all simple cycles in the undirected graph using DFS-based cycle enumeration
/// Returns all valid cycles without filtering
pub fn find_all_cycles(graph: &FloorplanGraph) -> Vec<Vec<NodeIndex>> {
    let mut all_cycles = Vec::new();

    // For each node, start DFS to find cycles
    for start_node in graph.node_indices() {
        if all_cycles.len() >= MAX_CYCLES {
            debug!("Reached maximum cycle limit ({})", MAX_CYCLES);
            break;
        }

        debug!("Starting cycle detection from node {:?}", start_node);
        let cycles = find_cycles_from_node_dfs(graph, start_node);
        all_cycles.extend(cycles);
    }

    // Deduplicate cycles
    deduplicate_cycles(all_cycles)
}

/// Find cycles that could represent room boundaries (filtered version)
fn find_room_cycles(graph: &FloorplanGraph) -> Vec<Vec<NodeIndex>> {
    let all_cycles = find_all_cycles(graph);

    // Filter to only include cycles that are large enough to be rooms (exactly 4 sides)
    filter_room_cycles(all_cycles, graph)
}

/// Find all cycles starting from a given node using DFS
/// This enumerates all simple cycles reachable from the start node
fn find_cycles_from_node_dfs(
    graph: &FloorplanGraph,
    start: NodeIndex,
) -> Vec<Vec<NodeIndex>> {
    let mut cycles = Vec::new();
    let mut path = Vec::new();
    let mut path_set = HashSet::new();

    // Start DFS from the start node
    dfs_cycle_find(graph, start, &mut path, &mut path_set, &mut cycles);

    cycles
}

/// DFS function to find cycles
/// Only finds cycles that return to the start node (proper simple cycles)
fn dfs_cycle_find(
    graph: &FloorplanGraph,
    current: NodeIndex,
    path: &mut Vec<NodeIndex>,
    path_set: &mut HashSet<NodeIndex>,
    cycles: &mut Vec<Vec<NodeIndex>>,
) {
    // Add current node to path
    path.push(current);
    path_set.insert(current);

    // Explore neighbors
    for edge in graph.edges(current) {
        // For undirected graph, get the "other" node (not current)
        let neighbor = if edge.source() == current {
            edge.target()
        } else {
            edge.source()
        };

        if !path_set.contains(&neighbor) {
            // Neighbor not in current path, continue DFS
            dfs_cycle_find(graph, neighbor, path, path_set, cycles);
        } else if path.len() >= 3 && neighbor == path[0] {
            // Found cycle back to start node - validate it's a proper cycle
            let mut cycle = path.clone();
            cycle.push(path[0]); // Close the cycle
            if cycle.len() <= MAX_CYCLE_LENGTH && is_valid_cycle(&cycle, graph) {
                cycles.push(cycle);
            }
        }
        // Ignore other back edges - they don't form valid simple cycles from the start
    }

    // Backtrack
    path.pop();
    path_set.remove(&current);
}

/// Reconstruct a cycle path from start to end using parent map
fn reconstruct_cycle(
    start: NodeIndex,
    end: NodeIndex,
    parent_map: &HashMap<NodeIndex, NodeIndex>,
) -> Vec<NodeIndex> {
    let mut path = vec![start];
    let mut current = end;

    // Trace back from end to start
    while current != start {
        path.push(current);
        if let Some(&parent) = parent_map.get(&current) {
            current = parent;
        } else {
            // No path found, return empty
            warn!("Failed to reconstruct cycle: no parent found for node {:?} when tracing from {:?} to {:?}", current, end, start);
            return Vec::new();
        }

        // Safety check for infinite loops
        if path.len() > MAX_CYCLE_LENGTH {
            warn!("Cycle reconstruction exceeded maximum length ({}) for cycle from {:?} to {:?}", MAX_CYCLE_LENGTH, start, end);
            return Vec::new();
        }
    }

    path.reverse();
    path
}

/// Check if a cycle is valid (all consecutive nodes are connected by edges)
fn is_valid_cycle(cycle: &[NodeIndex], graph: &FloorplanGraph) -> bool {
    if cycle.len() < 3 {
        return false;
    }

    for i in 0..cycle.len() - 1 {
        let a = cycle[i];
        let b = cycle[i + 1];

        // Check if b is a neighbor of a
        let mut found = false;
        for edge in graph.edges(a) {
            let neighbor = if edge.source() == a { edge.target() } else { edge.source() };
            if neighbor == b {
                found = true;
                break;
            }
        }

        if !found {
            return false;
        }
    }

    true
}

/// Filter cycles to only include those that represent potential room boundaries
/// - Must have exactly 4 sides (most rooms are rectangular)
/// - Must be valid (all edges exist)
fn filter_room_cycles(cycles: Vec<Vec<NodeIndex>>, graph: &FloorplanGraph) -> Vec<Vec<NodeIndex>> {
    let mut filtered_cycles: Vec<Vec<NodeIndex>> = Vec::new();

    for cycle in cycles {
        // Must be valid
        if !is_valid_cycle(&cycle, graph) {
            continue;
        }

        // Must have exactly 4 sides (excluding closing node) - typical for rectangular rooms
        let cycle_len = if cycle.len() > 1 && cycle[0] == cycle[cycle.len() - 1] {
            cycle.len() - 1 // Don't count closing node
        } else {
            cycle.len()
        };

        if cycle_len == 4 {
            filtered_cycles.push(cycle);
        }
    }

    filtered_cycles
}



/// Deduplicate cycles that represent the same room
/// Handles cycles with different starting points and reverse traversals
fn deduplicate_cycles(cycles: Vec<Vec<NodeIndex>>) -> Vec<Vec<NodeIndex>> {
    let mut unique_cycles = Vec::new();
    let mut seen_signatures = HashSet::new();

    for cycle in cycles {
        if cycle.len() < 3 {
            continue; // Skip invalid cycles
        }

        let signature = cycle_signature(&cycle);

        if !seen_signatures.contains(&signature) {
            seen_signatures.insert(signature);
            unique_cycles.push(cycle);
        }
    }

    unique_cycles
}

/// Create a canonical signature for a cycle to identify duplicates
/// Handles different starting points and reverse traversals
fn cycle_signature(cycle: &[NodeIndex]) -> Vec<u32> {
    if cycle.is_empty() {
        return Vec::new();
    }

    let mut indices: Vec<u32> = cycle.iter().map(|n| n.index() as u32).collect();

    // If the cycle has a closing node (last == first), remove it for signature calculation
    if indices.len() > 1 && indices[0] == indices[indices.len() - 1] {
        indices.pop();
    }

    // For cycles, we need to consider all rotations and find the lexicographically smallest
    // Also consider the reverse direction
    let n = indices.len();
    let mut candidates = Vec::new();

    // All rotations of forward direction
    for i in 0..n {
        let rotated: Vec<u32> = indices[i..].iter().chain(indices[..i].iter()).copied().collect();
        candidates.push(rotated);
    }

    // All rotations of reverse direction
    let mut reversed = indices.clone();
    reversed.reverse();
    for i in 0..n {
        let rotated: Vec<u32> = reversed[i..].iter().chain(reversed[..i].iter()).copied().collect();
        candidates.push(rotated);
    }

    // Return the lexicographically smallest candidate
    candidates.into_iter().min().unwrap()
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

    #[test]
    fn test_cycle_detection_multiple_rooms() {
        // Create two adjacent rooms sharing a wall
        let lines = vec![
            // Room 1 (left square)
            Line { start: Point { x: 0.0, y: 0.0 }, end: Point { x: 100.0, y: 0.0 }, is_load_bearing: false },
            Line { start: Point { x: 100.0, y: 0.0 }, end: Point { x: 100.0, y: 100.0 }, is_load_bearing: false },
            Line { start: Point { x: 100.0, y: 100.0 }, end: Point { x: 0.0, y: 100.0 }, is_load_bearing: false },
            Line { start: Point { x: 0.0, y: 100.0 }, end: Point { x: 0.0, y: 0.0 }, is_load_bearing: false },
            // Room 2 (right square)
            Line { start: Point { x: 100.0, y: 0.0 }, end: Point { x: 200.0, y: 0.0 }, is_load_bearing: false },
            Line { start: Point { x: 200.0, y: 0.0 }, end: Point { x: 200.0, y: 100.0 }, is_load_bearing: false },
            Line { start: Point { x: 200.0, y: 100.0 }, end: Point { x: 100.0, y: 100.0 }, is_load_bearing: false },
        ];

        let graph = build_graph(&lines);
        let cycles = find_all_cycles(&graph);

        // Should find all valid cycles in the graph
        assert_eq!(cycles.len(), 3, "Should detect all valid cycles in the graph");

        // Check that we have the expected cycle lengths
        let cycle_lengths: Vec<usize> = cycles.iter().map(|c| c.len()).collect();
        assert!(cycle_lengths.contains(&5), "Should have 5-node cycles");
        assert!(cycle_lengths.contains(&7), "Should have 7-node cycle");
    }

    #[test]
    fn test_cycle_detection_no_duplicates() {
        // Create a simple square and verify no duplicate cycles are found
        let lines = vec![
            Line { start: Point { x: 0.0, y: 0.0 }, end: Point { x: 100.0, y: 0.0 }, is_load_bearing: false },
            Line { start: Point { x: 100.0, y: 0.0 }, end: Point { x: 100.0, y: 100.0 }, is_load_bearing: false },
            Line { start: Point { x: 100.0, y: 100.0 }, end: Point { x: 0.0, y: 100.0 }, is_load_bearing: false },
            Line { start: Point { x: 0.0, y: 100.0 }, end: Point { x: 0.0, y: 0.0 }, is_load_bearing: false },
        ];

        let graph = build_graph(&lines);
        let cycles = find_all_cycles(&graph);

        // Should find exactly 1 cycle (the square)
        assert_eq!(cycles.len(), 1, "Should detect exactly 1 cycle for a simple square");
        assert_eq!(cycles[0].len(), 5, "Cycle should have 5 nodes (including closing)");
    }

    #[test]
    fn test_cycle_detection_complex_floorplan() {
        // Create an L-shaped floorplan with two rooms
        let lines = vec![
            // Outer boundary
            Line { start: Point { x: 0.0, y: 0.0 }, end: Point { x: 200.0, y: 0.0 }, is_load_bearing: false },
            Line { start: Point { x: 200.0, y: 0.0 }, end: Point { x: 200.0, y: 100.0 }, is_load_bearing: false },
            Line { start: Point { x: 200.0, y: 100.0 }, end: Point { x: 100.0, y: 100.0 }, is_load_bearing: false },
            Line { start: Point { x: 100.0, y: 100.0 }, end: Point { x: 100.0, y: 200.0 }, is_load_bearing: false },
            Line { start: Point { x: 100.0, y: 200.0 }, end: Point { x: 0.0, y: 200.0 }, is_load_bearing: false },
            Line { start: Point { x: 0.0, y: 200.0 }, end: Point { x: 0.0, y: 0.0 }, is_load_bearing: false },
            // Internal wall
            Line { start: Point { x: 100.0, y: 0.0 }, end: Point { x: 100.0, y: 100.0 }, is_load_bearing: false },
        ];

        let graph = build_graph(&lines);
        let cycles = find_all_cycles(&graph);

        // Should find the outer boundary cycle
        assert_eq!(cycles.len(), 1, "Should detect the outer boundary cycle");
        assert_eq!(cycles[0].len(), 7, "Outer boundary cycle should have 7 nodes (including closing)");
    }

    #[test]
    fn test_cycle_detection_empty_graph() {
        let lines = vec![];
        let graph = build_graph(&lines);
        let cycles = find_all_cycles(&graph);

        assert_eq!(cycles.len(), 0, "Empty graph should have no cycles");
    }

    #[test]
    fn test_cycle_detection_single_line() {
        let lines = vec![
            Line { start: Point { x: 0.0, y: 0.0 }, end: Point { x: 100.0, y: 0.0 }, is_load_bearing: false },
        ];
        let graph = build_graph(&lines);
        let cycles = find_all_cycles(&graph);

        assert_eq!(cycles.len(), 0, "Single line should have no cycles");
    }

    #[test]
    fn test_cycle_detection_triangle() {
        let lines = vec![
            Line { start: Point { x: 0.0, y: 0.0 }, end: Point { x: 100.0, y: 0.0 }, is_load_bearing: false },
            Line { start: Point { x: 100.0, y: 0.0 }, end: Point { x: 50.0, y: 86.6 }, is_load_bearing: false },
            Line { start: Point { x: 50.0, y: 86.6 }, end: Point { x: 0.0, y: 0.0 }, is_load_bearing: false },
        ];

        let graph = build_graph(&lines);
        let cycles = find_all_cycles(&graph);

        assert_eq!(cycles.len(), 1, "Triangle should have exactly 1 cycle");
        assert_eq!(cycles[0].len(), 4, "Triangle cycle should have 4 nodes (including closing)");
    }

    #[test]
    fn test_deduplicate_cycles() {
        // Create cycles that are the same but with different starting points
        let cycle1 = vec![
            NodeIndex::new(0),
            NodeIndex::new(1),
            NodeIndex::new(2),
            NodeIndex::new(3),
        ];
        let cycle2 = vec![
            NodeIndex::new(1),
            NodeIndex::new(2),
            NodeIndex::new(3),
            NodeIndex::new(0),
        ]; // Same cycle, different starting point
        let cycle3 = vec![
            NodeIndex::new(3),
            NodeIndex::new(2),
            NodeIndex::new(1),
            NodeIndex::new(0),
        ]; // Reverse of cycle1

        let cycles = vec![cycle1, cycle2, cycle3];
        let deduplicated = deduplicate_cycles(cycles);

        assert_eq!(deduplicated.len(), 1, "Should deduplicate to exactly 1 unique cycle");
    }

    #[test]
    fn test_cycle_signature() {
        let cycle = vec![
            NodeIndex::new(3),
            NodeIndex::new(0),
            NodeIndex::new(1),
            NodeIndex::new(2),
        ];

        let signature = cycle_signature(&cycle);
        let expected = vec![0u32, 1, 2, 3]; // Should start with minimum element

        assert_eq!(signature, expected, "Signature should be normalized to start with minimum element");
    }
}
