use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tracing::info;

/// Line segment representing a wall
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Line {
    pub start: Point,
    pub end: Point,
    #[serde(default)]
    pub is_load_bearing: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>, // "vtracer", "gpt5", or "consensus"
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    fn distance_to(&self, other: &Point) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }
}

/// Result of merging wall segments from multiple sources
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeResult {
    pub walls: Vec<Line>,
    pub metadata: MergeMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeMetadata {
    pub vtracer_count: usize,
    pub gpt5_count: usize,
    pub merged_count: usize,
    pub consensus_count: usize,
    pub strategy_used: String,
}

/// Merge strategy for combining wall segments from different sources
#[derive(Debug, Clone, Copy)]
pub enum MergeStrategy {
    /// Use GPT-5 as primary, VTracer as supplementary
    VisionPrimary,
    /// Use VTracer as primary, GPT-5 as validation
    VtracerPrimary,
    /// Combine both with consensus voting
    Consensus,
}

impl MergeStrategy {
    pub fn from_confidence(vision_confidence: f64, threshold: f64) -> Self {
        if vision_confidence >= threshold {
            MergeStrategy::VisionPrimary
        } else if vision_confidence < 0.4 {
            MergeStrategy::VtracerPrimary
        } else {
            MergeStrategy::Consensus
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            MergeStrategy::VisionPrimary => "vision_primary",
            MergeStrategy::VtracerPrimary => "vtracer_primary",
            MergeStrategy::Consensus => "consensus",
        }
    }
}

/// Merge wall segments from VTracer and GPT-5 Vision
///
/// # Arguments
/// * `vtracer_walls` - Wall segments from VTracer geometric vectorization
/// * `vision_walls` - Wall segments from GPT-5 Vision API
/// * `vision_confidence` - Confidence score from GPT-5 (0.0-1.0)
/// * `threshold` - Confidence threshold for strategy selection (default: 0.75)
///
/// # Returns
/// Merged wall segments with metadata about the merge process
pub fn merge_wall_segments(
    vtracer_walls: Vec<Line>,
    vision_walls: Vec<Line>,
    vision_confidence: f64,
    threshold: f64,
) -> MergeResult {
    let strategy = MergeStrategy::from_confidence(vision_confidence, threshold);

    info!(
        "Merging {} VTracer walls and {} GPT-5 walls using {:?} strategy (confidence: {:.2})",
        vtracer_walls.len(),
        vision_walls.len(),
        strategy,
        vision_confidence
    );

    let (walls, consensus_count) = match strategy {
        MergeStrategy::VisionPrimary => {
            merge_vision_primary(&vtracer_walls, &vision_walls)
        }
        MergeStrategy::VtracerPrimary => {
            merge_vtracer_primary(&vtracer_walls, &vision_walls)
        }
        MergeStrategy::Consensus => {
            merge_consensus(&vtracer_walls, &vision_walls)
        }
    };

    let metadata = MergeMetadata {
        vtracer_count: vtracer_walls.len(),
        gpt5_count: vision_walls.len(),
        merged_count: walls.len(),
        consensus_count,
        strategy_used: strategy.as_str().to_string(),
    };

    info!(
        "Merge complete: {} walls ({} consensus)",
        metadata.merged_count, metadata.consensus_count
    );

    MergeResult { walls, metadata }
}

/// Merge with GPT-5 Vision as primary source
/// Uses VTracer walls to supplement areas GPT-5 might have missed
fn merge_vision_primary(vtracer_walls: &[Line], vision_walls: &[Line]) -> (Vec<Line>, usize) {
    let mut result = Vec::new();
    let mut used_vtracer = HashSet::new();
    let mut consensus_count = 0;

    // Add all GPT-5 walls first
    for wall in vision_walls.iter() {
        let mut wall_clone = wall.clone();
        wall_clone.source = Some("gpt5".to_string());

        // Check if this wall is confirmed by VTracer (consensus)
        if let Some(vtracer_idx) = find_similar_wall(wall, vtracer_walls, 5.0) {
            wall_clone.source = Some("consensus".to_string());
            used_vtracer.insert(vtracer_idx);
            consensus_count += 1;
        }

        result.push(wall_clone);
    }

    // Add VTracer walls that weren't matched (supplementary walls)
    for (i, wall) in vtracer_walls.iter().enumerate() {
        if !used_vtracer.contains(&i) {
            let mut wall_clone = wall.clone();
            wall_clone.source = Some("vtracer".to_string());
            result.push(wall_clone);
        }
    }

    (result, consensus_count)
}

/// Merge with VTracer as primary source
/// Uses GPT-5 walls for validation and filling gaps
fn merge_vtracer_primary(vtracer_walls: &[Line], vision_walls: &[Line]) -> (Vec<Line>, usize) {
    let mut result = Vec::new();
    let mut used_vision = HashSet::new();
    let mut consensus_count = 0;

    // Add all VTracer walls first
    for wall in vtracer_walls.iter() {
        let mut wall_clone = wall.clone();
        wall_clone.source = Some("vtracer".to_string());

        // Check if this wall is confirmed by GPT-5 (consensus)
        if let Some(vision_idx) = find_similar_wall(wall, vision_walls, 5.0) {
            wall_clone.source = Some("consensus".to_string());
            used_vision.insert(vision_idx);
            consensus_count += 1;
        }

        result.push(wall_clone);
    }

    // Add GPT-5 walls that weren't matched (validation/gap-filling)
    for (idx, wall) in vision_walls.iter().enumerate() {
        if !used_vision.contains(&idx) {
            let mut wall_clone = wall.clone();
            wall_clone.source = Some("gpt5".to_string());
            result.push(wall_clone);
        }
    }

    (result, consensus_count)
}

/// Merge with consensus voting - only keep walls that appear in both sources or are very confident
fn merge_consensus(vtracer_walls: &[Line], vision_walls: &[Line]) -> (Vec<Line>, usize) {
    let mut result = Vec::new();
    let mut used_vtracer = HashSet::new();
    let mut used_vision = HashSet::new();
    let mut consensus_count = 0;

    // Find consensus walls (appear in both sources)
    for (v_idx, vtracer_wall) in vtracer_walls.iter().enumerate() {
        if let Some(vision_idx) = find_similar_wall(vtracer_wall, vision_walls, 5.0) {
            // This wall appears in both sources - high confidence
            let mut wall = vtracer_wall.clone();
            wall.source = Some("consensus".to_string());
            result.push(wall);
            used_vtracer.insert(v_idx);
            used_vision.insert(vision_idx);
            consensus_count += 1;
        }
    }

    // Add high-confidence VTracer walls (long walls are likely structural)
    for (v_idx, wall) in vtracer_walls.iter().enumerate() {
        if !used_vtracer.contains(&v_idx) && wall_length(wall) > 50.0 {
            let mut wall_clone = wall.clone();
            wall_clone.source = Some("vtracer".to_string());
            result.push(wall_clone);
        }
    }

    // Add high-confidence GPT-5 walls (long walls)
    for (g_idx, wall) in vision_walls.iter().enumerate() {
        if !used_vision.contains(&g_idx) && wall_length(wall) > 50.0 {
            let mut wall_clone = wall.clone();
            wall_clone.source = Some("gpt5".to_string());
            result.push(wall_clone);
        }
    }

    (result, consensus_count)
}

/// Find a similar wall in a list of walls within a tolerance
/// Returns the index of the first similar wall found
fn find_similar_wall(wall: &Line, walls: &[Line], tolerance: f64) -> Option<usize> {
    walls.iter().position(|other| {
        are_walls_similar(wall, other, tolerance)
    })
}

/// Check if two walls are similar (within tolerance)
fn are_walls_similar(wall1: &Line, wall2: &Line, tolerance: f64) -> bool {
    // Check if start points match and end points match
    let start_match = wall1.start.distance_to(&wall2.start) < tolerance
        && wall1.end.distance_to(&wall2.end) < tolerance;

    // Check if reversed (start matches end and vice versa)
    let reversed_match = wall1.start.distance_to(&wall2.end) < tolerance
        && wall1.end.distance_to(&wall2.start) < tolerance;

    start_match || reversed_match
}

/// Calculate the length of a wall segment
fn wall_length(wall: &Line) -> f64 {
    wall.start.distance_to(&wall.end)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_line(x1: f64, y1: f64, x2: f64, y2: f64) -> Line {
        Line {
            start: Point { x: x1, y: y1 },
            end: Point { x: x2, y: y2 },
            is_load_bearing: false,
            source: None,
        }
    }

    #[test]
    fn test_similar_walls_exact_match() {
        let wall1 = create_line(0.0, 0.0, 100.0, 0.0);
        let wall2 = create_line(0.0, 0.0, 100.0, 0.0);
        assert!(are_walls_similar(&wall1, &wall2, 5.0));
    }

    #[test]
    fn test_similar_walls_reversed() {
        let wall1 = create_line(0.0, 0.0, 100.0, 0.0);
        let wall2 = create_line(100.0, 0.0, 0.0, 0.0);
        assert!(are_walls_similar(&wall1, &wall2, 5.0));
    }

    #[test]
    fn test_similar_walls_within_tolerance() {
        let wall1 = create_line(0.0, 0.0, 100.0, 0.0);
        let wall2 = create_line(1.0, 1.0, 101.0, 1.0);
        assert!(are_walls_similar(&wall1, &wall2, 5.0));
    }

    #[test]
    fn test_dissimilar_walls() {
        let wall1 = create_line(0.0, 0.0, 100.0, 0.0);
        let wall2 = create_line(0.0, 0.0, 0.0, 100.0);
        assert!(!are_walls_similar(&wall1, &wall2, 5.0));
    }

    #[test]
    fn test_merge_vision_primary_high_confidence() {
        let vtracer_walls = vec![
            create_line(0.0, 0.0, 100.0, 0.0),
            create_line(100.0, 0.0, 100.0, 100.0),
        ];

        let vision_walls = vec![
            create_line(0.0, 0.0, 100.0, 0.0), // Matches first VTracer wall
            create_line(0.0, 100.0, 100.0, 100.0), // New wall from GPT-5
        ];

        let result = merge_wall_segments(vtracer_walls, vision_walls, 0.85, 0.75);

        assert_eq!(result.metadata.strategy_used, "vision_primary");
        assert_eq!(result.metadata.consensus_count, 1); // One wall matched
        assert_eq!(result.walls.len(), 3); // 2 GPT-5 + 1 unmatched VTracer
    }

    #[test]
    fn test_merge_vtracer_primary_low_confidence() {
        let vtracer_walls = vec![
            create_line(0.0, 0.0, 100.0, 0.0),
            create_line(100.0, 0.0, 100.0, 100.0),
        ];

        let vision_walls = vec![
            create_line(0.0, 0.0, 100.0, 0.0),
        ];

        let result = merge_wall_segments(vtracer_walls, vision_walls, 0.3, 0.75);

        assert_eq!(result.metadata.strategy_used, "vtracer_primary");
        assert_eq!(result.metadata.consensus_count, 1);
        assert_eq!(result.walls.len(), 2); // Both VTracer walls (one is consensus)
    }

    #[test]
    fn test_merge_consensus_medium_confidence() {
        let vtracer_walls = vec![
            create_line(0.0, 0.0, 100.0, 0.0), // Will match
            create_line(0.0, 0.0, 0.0, 10.0), // Short wall, won't be included
        ];

        let vision_walls = vec![
            create_line(0.0, 0.0, 100.0, 0.0), // Will match
            create_line(200.0, 0.0, 200.0, 10.0), // Short wall, won't be included
        ];

        let result = merge_wall_segments(vtracer_walls, vision_walls, 0.6, 0.75);

        assert_eq!(result.metadata.strategy_used, "consensus");
        assert_eq!(result.metadata.consensus_count, 1);
        // Only consensus walls and walls > 50 units
    }

    #[test]
    fn test_wall_length() {
        let wall = create_line(0.0, 0.0, 3.0, 4.0);
        assert_eq!(wall_length(&wall), 5.0); // 3-4-5 triangle
    }
}
