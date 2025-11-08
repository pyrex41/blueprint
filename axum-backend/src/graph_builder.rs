use crate::{Line, Point, PointKey};
use petgraph::graph::{NodeIndex, UnGraph};
use std::collections::HashMap;

// Use UnGraph (undirected graph) for floorplan lines since walls connect points bidirectionally
pub type FloorplanGraph = UnGraph<Point, EdgeType>;

/// Edge types in the floorplan graph
#[derive(Debug, Clone)]
pub enum EdgeType {
    Wall(Line),           // Actual wall from input
    VirtualDoor(Line),    // Virtual connection across a door gap
}

impl EdgeType {
    pub fn is_virtual(&self) -> bool {
        matches!(self, EdgeType::VirtualDoor(_))
    }

    pub fn line(&self) -> &Line {
        match self {
            EdgeType::Wall(line) | EdgeType::VirtualDoor(line) => line,
        }
    }
}

/// Build an undirected graph from a list of lines
/// Nodes represent points, edges represent lines connecting them bidirectionally
pub fn build_graph(lines: &[Line]) -> FloorplanGraph {
    build_graph_with_door_threshold(lines, 0.0)
}

/// Build an undirected graph with door gap detection
///
/// # Arguments
/// * `lines` - Wall line segments
/// * `door_threshold` - Maximum distance to treat as a door gap (0.0 = no gap bridging)
///
/// # Returns
/// Graph where edges are either real walls or virtual door connections
pub fn build_graph_with_door_threshold(lines: &[Line], door_threshold: f64) -> FloorplanGraph {
    let mut graph = UnGraph::new_undirected();
    let mut point_to_node: HashMap<PointKey, NodeIndex> = HashMap::new();

    // Phase 1: Add all real wall segments
    for line in lines {
        // Skip degenerate lines (start == end)
        if line.start == line.end {
            continue;
        }

        // Get or create nodes for start and end points
        let start_key = PointKey::from(&line.start);
        let end_key = PointKey::from(&line.end);

        let start_node = *point_to_node
            .entry(start_key)
            .or_insert_with(|| graph.add_node(line.start.clone()));

        let end_node = *point_to_node
            .entry(end_key)
            .or_insert_with(|| graph.add_node(line.end.clone()));

        // Add undirected edge for wall
        graph.add_edge(start_node, end_node, EdgeType::Wall(line.clone()));
    }

    // Phase 2: Bridge small gaps (potential doors)
    if door_threshold > 0.0 {
        bridge_door_gaps(&mut graph, &point_to_node, door_threshold);
    }

    graph
}

/// Find nearby points that could represent door openings and connect them
fn bridge_door_gaps(
    graph: &mut FloorplanGraph,
    point_to_node: &HashMap<PointKey, NodeIndex>,
    threshold: f64,
) {
    let nodes: Vec<_> = graph.node_indices().collect();
    let mut gaps_to_bridge = Vec::new();

    // Find all pairs of nearby points that aren't already connected
    for i in 0..nodes.len() {
        for j in (i + 1)..nodes.len() {
            let node_i = nodes[i];
            let node_j = nodes[j];

            // Skip if already connected
            if graph.find_edge(node_i, node_j).is_some() {
                continue;
            }

            let point_i = &graph[node_i];
            let point_j = &graph[node_j];

            let distance = point_i.distance_to(point_j);

            // If points are close enough, they might represent a door opening
            if distance > 0.0 && distance <= threshold {
                // Create virtual door connection
                let virtual_line = Line {
                    start: point_i.clone(),
                    end: point_j.clone(),
                    is_load_bearing: false,
                };

                gaps_to_bridge.push((node_i, node_j, virtual_line));
            }
        }
    }

    // Add virtual door edges
    for (node_i, node_j, virtual_line) in gaps_to_bridge {
        graph.add_edge(node_i, node_j, EdgeType::VirtualDoor(virtual_line));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_graph_construction() {
        let lines = vec![
            Line {
                start: Point { x: 0.0, y: 0.0 },
                end: Point { x: 1.0, y: 0.0 },
                is_load_bearing: false,
            },
            Line {
                start: Point { x: 1.0, y: 0.0 },
                end: Point { x: 1.0, y: 1.0 },
                is_load_bearing: false,
            },
        ];

        let graph = build_graph(&lines);

        assert_eq!(graph.node_count(), 3); // 3 unique points
        assert_eq!(graph.edge_count(), 2); // 2 lines
    }

    #[test]
    fn test_graph_with_shared_points() {
        let shared_point = Point { x: 5.0, y: 5.0 };

        let lines = vec![
            Line {
                start: Point { x: 0.0, y: 0.0 },
                end: shared_point.clone(),
                is_load_bearing: false,
            },
            Line {
                start: shared_point.clone(),
                end: Point { x: 10.0, y: 0.0 },
                is_load_bearing: false,
            },
            Line {
                start: shared_point.clone(),
                end: Point { x: 5.0, y: 10.0 },
                is_load_bearing: false,
            },
        ];

        let graph = build_graph(&lines);

        // Should have 4 unique points, with the shared point appearing once
        assert_eq!(graph.node_count(), 4);
        assert_eq!(graph.edge_count(), 3);
    }

    #[test]
    fn test_empty_lines() {
        let lines: Vec<Line> = vec![];
        let graph = build_graph(&lines);

        assert_eq!(graph.node_count(), 0);
        assert_eq!(graph.edge_count(), 0);
    }
}
