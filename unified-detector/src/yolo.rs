/// YOLO detector interface for ONNX model inference
///
/// This module provides a trait-based interface for YOLO detection.
/// The actual ONNX runtime implementation will be added once the model is trained.

use crate::{Room, DetectionResult, DetectionMethod, DetectionMetadata};
use std::path::Path;

/// YOLO detection configuration
#[derive(Debug, Clone)]
pub struct YoloConfig {
    /// Path to ONNX model file
    pub model_path: String,
    /// Confidence threshold (0.0-1.0)
    pub confidence_threshold: f64,
    /// NMS IoU threshold (0.0-1.0)
    pub nms_threshold: f64,
    /// Input image size (width, height)
    pub input_size: (u32, u32),
}

impl Default for YoloConfig {
    fn default() -> Self {
        Self {
            model_path: "yolo-training/runs/detect/train/weights/best.onnx".to_string(),
            confidence_threshold: 0.25,
            nms_threshold: 0.45,
            input_size: (640, 640),
        }
    }
}

/// YOLO bounding box detection
#[derive(Debug, Clone)]
pub struct YoloDetection {
    pub class_id: usize,
    pub class_name: String,
    pub confidence: f32,
    pub bbox: [f32; 4], // [x1, y1, x2, y2]
}

/// YOLO detector trait - allows for different implementations
pub trait YoloDetector: Send + Sync {
    /// Detect rooms in an image
    fn detect(&self, image_bytes: &[u8]) -> anyhow::Result<Vec<YoloDetection>>;

    /// Get model info
    fn model_info(&self) -> String;
}

/// Stub implementation for when ONNX runtime is not available
pub struct StubYoloDetector {
    config: YoloConfig,
}

impl StubYoloDetector {
    pub fn new(config: YoloConfig) -> anyhow::Result<Self> {
        Ok(Self { config })
    }
}

impl YoloDetector for StubYoloDetector {
    fn detect(&self, _image_bytes: &[u8]) -> anyhow::Result<Vec<YoloDetection>> {
        Err(anyhow::anyhow!(
            "YOLO detector not available. Train model and add ort (ONNX Runtime) dependency."
        ))
    }

    fn model_info(&self) -> String {
        format!("Stub YOLO Detector (model path: {})", self.config.model_path)
    }
}

/// Check if YOLO model is available
pub fn is_yolo_available() -> bool {
    let default_path = YoloConfig::default().model_path;
    Path::new(&default_path).exists()
}

/// Convert YOLO detections to unified Room format
pub fn detections_to_rooms(detections: Vec<YoloDetection>) -> Vec<Room> {
    detections
        .into_iter()
        .enumerate()
        .map(|(idx, det)| {
            let bbox = [
                det.bbox[0] as f64,
                det.bbox[1] as f64,
                det.bbox[2] as f64,
                det.bbox[3] as f64,
            ];

            let area = (bbox[2] - bbox[0]) * (bbox[3] - bbox[1]);

            Room {
                id: idx,
                room_type: det.class_name.clone(),
                confidence: det.confidence as f64,
                area,
                bounding_box: bbox,
                features: vec![],
            }
        })
        .collect()
}

/// Create a detection result from YOLO detections
pub fn create_detection_result(
    detections: Vec<YoloDetection>,
    execution_time: std::time::Duration,
) -> DetectionResult {
    let rooms = detections_to_rooms(detections);

    let avg_confidence = if rooms.is_empty() {
        0.0
    } else {
        rooms.iter().map(|r| r.confidence).sum::<f64>() / rooms.len() as f64
    };

    let metadata = DetectionMetadata {
        total_rooms: rooms.len(),
        avg_confidence,
        method_specific: serde_json::json!({
            "detector_type": "YOLOv8",
            "model_format": "ONNX"
        }),
    };

    DetectionResult {
        method: DetectionMethod::YoloV8,
        rooms,
        execution_time,
        metadata,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = YoloConfig::default();
        assert_eq!(config.confidence_threshold, 0.25);
        assert_eq!(config.nms_threshold, 0.45);
        assert_eq!(config.input_size, (640, 640));
    }

    #[test]
    fn test_stub_detector() {
        let config = YoloConfig::default();
        let detector = StubYoloDetector::new(config).unwrap();

        // Should fail since it's a stub
        let result = detector.detect(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_detections_to_rooms() {
        let detections = vec![
            YoloDetection {
                class_id: 0,
                class_name: "bedroom".to_string(),
                confidence: 0.95,
                bbox: [10.0, 20.0, 110.0, 120.0],
            },
            YoloDetection {
                class_id: 1,
                class_name: "kitchen".to_string(),
                confidence: 0.88,
                bbox: [200.0, 50.0, 350.0, 200.0],
            },
        ];

        let rooms = detections_to_rooms(detections);

        assert_eq!(rooms.len(), 2);
        assert_eq!(rooms[0].room_type, "bedroom");
        assert_eq!(rooms[0].confidence, 0.95);
        assert_eq!(rooms[1].room_type, "kitchen");
        assert_eq!(rooms[1].confidence, 0.88);
    }
}
