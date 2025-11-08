use serde::{Deserialize, Serialize};
use std::time::Duration;

pub mod yolo;

/// Unified detection result from any method
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionResult {
    pub method: DetectionMethod,
    pub rooms: Vec<Room>,
    pub execution_time: Duration,
    pub metadata: DetectionMetadata,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DetectionMethod {
    GraphBased,
    GraphWithDoors,
    VisionGPT5,
    VisionGPT4,
    YoloV8,
    HuggingFace,
}

impl DetectionMethod {
    pub fn name(&self) -> &'static str {
        match self {
            Self::GraphBased => "Graph-Based",
            Self::GraphWithDoors => "Graph+Doors",
            Self::VisionGPT5 => "GPT-5 Vision",
            Self::VisionGPT4 => "GPT-4 Vision",
            Self::YoloV8 => "YOLOv8",
            Self::HuggingFace => "HuggingFace",
        }
    }

    pub fn is_available(&self) -> bool {
        match self {
            Self::GraphBased | Self::GraphWithDoors => true,
            Self::VisionGPT5 | Self::VisionGPT4 => {
                std::env::var("OPENAI_API_KEY").is_ok()
            }
            Self::YoloV8 | Self::HuggingFace => false, // Not implemented yet
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Room {
    pub id: usize,
    pub room_type: String,
    pub confidence: f64,
    pub area: f64,
    pub bounding_box: [f64; 4],
    pub features: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionMetadata {
    pub total_rooms: usize,
    pub avg_confidence: f64,
    pub method_specific: serde_json::Value,
}

/// Ensemble configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnsembleConfig {
    pub methods: Vec<DetectionMethod>,
    pub strategy: EnsembleStrategy,
    pub parallel: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum EnsembleStrategy {
    /// Take results from fastest method
    Fastest,
    /// Take results from most accurate method (based on confidence)
    HighestConfidence,
    /// Combine results with voting by area overlap
    VoteByArea,
    /// Run methods in cascade (fast → slow until confident)
    Cascade { confidence_threshold: f64 },
    /// Return all results for manual comparison
    All,
}

/// Benchmark result for a single method on a single image
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    pub method: DetectionMethod,
    pub image_path: String,
    pub success: bool,
    pub execution_time: Duration,
    pub rooms_detected: usize,
    pub avg_confidence: f64,
    pub error: Option<String>,
}

/// Aggregate benchmark statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkStats {
    pub method: DetectionMethod,
    pub total_tests: usize,
    pub successful: usize,
    pub failed: usize,
    pub avg_execution_time: Duration,
    pub min_execution_time: Duration,
    pub max_execution_time: Duration,
    pub avg_rooms_per_image: f64,
    pub avg_confidence: f64,
}

impl BenchmarkStats {
    pub fn from_results(method: DetectionMethod, results: &[BenchmarkResult]) -> Self {
        let method_results: Vec<_> = results
            .iter()
            .filter(|r| r.method == method)
            .collect();

        let successful: Vec<_> = method_results
            .iter()
            .filter(|r| r.success)
            .copied()
            .collect();

        let total = method_results.len();
        let success_count = successful.len();

        let avg_time = if !successful.is_empty() {
            let total_nanos: u128 = successful
                .iter()
                .map(|r| r.execution_time.as_nanos())
                .sum();
            Duration::from_nanos((total_nanos / successful.len() as u128) as u64)
        } else {
            Duration::from_secs(0)
        };

        let min_time = successful
            .iter()
            .map(|r| r.execution_time)
            .min()
            .unwrap_or(Duration::from_secs(0));

        let max_time = successful
            .iter()
            .map(|r| r.execution_time)
            .max()
            .unwrap_or(Duration::from_secs(0));

        let avg_rooms = if !successful.is_empty() {
            successful.iter().map(|r| r.rooms_detected).sum::<usize>() as f64
                / successful.len() as f64
        } else {
            0.0
        };

        let avg_conf = if !successful.is_empty() {
            successful.iter().map(|r| r.avg_confidence).sum::<f64>()
                / successful.len() as f64
        } else {
            0.0
        };

        Self {
            method,
            total_tests: total,
            successful: success_count,
            failed: total - success_count,
            avg_execution_time: avg_time,
            min_execution_time: min_time,
            max_execution_time: max_time,
            avg_rooms_per_image: avg_rooms,
            avg_confidence: avg_conf,
        }
    }

    pub fn success_rate(&self) -> f64 {
        if self.total_tests == 0 {
            0.0
        } else {
            (self.successful as f64 / self.total_tests as f64) * 100.0
        }
    }
}

/// Comparison report between methods
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonReport {
    pub image_path: String,
    pub results: Vec<DetectionResult>,
    pub winner: DetectionMethod,
    pub ranking: Vec<(DetectionMethod, f64)>, // (method, score)
}

impl ComparisonReport {
    pub fn new(image_path: String, results: Vec<DetectionResult>) -> Self {
        let mut ranking: Vec<(DetectionMethod, f64)> = results
            .iter()
            .map(|r| {
                // Score = (confidence * 0.7) + (1.0 / execution_time_seconds * 0.3)
                let conf_score = r.metadata.avg_confidence * 0.7;
                let speed_score = (1.0 / r.execution_time.as_secs_f64().max(0.001)) * 0.3;
                (r.method, conf_score + speed_score)
            })
            .collect();

        ranking.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        let winner = ranking.first().map(|(m, _)| *m).unwrap_or(DetectionMethod::GraphBased);

        Self {
            image_path,
            results,
            winner,
            ranking,
        }
    }
}
