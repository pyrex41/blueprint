use crate::{Line, Point};
use petgraph::graph::{Graph, NodeIndex};
use std::collections::HashMap;

pub type FloorplanGraph = Graph<Point, Line>;

/// Build a graph from a list of lines
/// Nodes represent points, edges represent lines connecting them
pub fn build_graph(lines: &[Line]) -> FloorplanGraph {
    let mut graph = Graph::new();
    let mut point_to_node: HashMap<PointKey, NodeIndex> = HashMap::new();

    for line in lines {
        // Get or create nodes for start and end points
        let start_key = PointKey::from(&line.start);
        let end_key = PointKey::from(&line.end);

        let start_node = *point_to_node
            .entry(start_key)
            .or_insert_with(|| graph.add_node(line.start.clone()));

        let end_node = *point_to_node
            .entry(end_key)
            .or_insert_with(|| graph.add_node(line.end.clone()));

        // Add edge between nodes
        graph.add_edge(start_node, end_node, line.clone());
    }

    graph
}

/// Key for hashing points in HashMap
/// Uses rounded coordinates to handle floating point precision
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct PointKey {
    x: i64,
    y: i64,
}

impl From<&Point> for PointKey {
    fn from(point: &Point) -> Self {
        PointKey {
            x: (point.x * 1_000_000.0) as i64,
            y: (point.y * 1_000_000.0) as i64,
        }
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
