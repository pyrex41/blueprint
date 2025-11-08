use crate::{Line, Room};
use serde::{Deserialize, Serialize};
use std::time::Instant;
use tracing::{info, warn};
use unified_detector::yolo::YoloDetector;

/// Unified detector that orchestrates multiple detection methods
pub struct DetectorOrchestrator {
    /// Configuration for detection
    config: DetectorConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectorConfig {
    /// Area threshold for geometric detection
    pub area_threshold: f64,
    /// Door gap threshold
    pub door_threshold: f64,
    /// Enable vision-based classification
    pub enable_vision: bool,
    /// Enable YOLO detection (when model is available)
    pub enable_yolo: bool,
    /// Strategy for combining results
    pub strategy: CombinationStrategy,
}

impl Default for DetectorConfig {
    fn default() -> Self {
        Self {
            area_threshold: 100.0,
            door_threshold: 50.0,
            enable_vision: false, // Disabled by default (requires API key)
            enable_yolo: false,   // Disabled until model is trained
            strategy: CombinationStrategy::GraphOnly,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum CombinationStrategy {
    /// Use only graph-based geometric detection
    GraphOnly,
    /// Use graph detection + vision classification
    GraphWithVision,
    /// Use YOLO detection only
    YoloOnly,
    /// Use best available method (fallback chain)
    BestAvailable,
    /// Run all methods and compare
    Ensemble,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionResult {
    pub rooms: Vec<EnhancedRoom>,
    pub method_used: String,
    pub execution_time_ms: u128,
    pub metadata: DetectionMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnhancedRoom {
    /// Base geometric room detection
    #[serde(flatten)]
    pub room: Room,
    /// Vision-based classification (if available)
    pub room_type: Option<String>,
    /// Confidence score from vision classifier
    pub confidence: Option<f64>,
    /// Features identified by vision
    pub features: Vec<String>,
    /// Detection method used
    pub detection_method: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionMetadata {
    pub graph_based_rooms: usize,
    pub vision_classified: usize,
    pub yolo_detected: usize,
    pub total_execution_time_ms: u128,
    pub method_timings: Vec<(String, u128)>,
}

impl DetectorOrchestrator {
    pub fn new(config: DetectorConfig) -> Self {
        Self { config }
    }

    pub fn with_defaults() -> Self {
        Self::new(DetectorConfig::default())
    }

    /// Detect rooms using the configured strategy
    pub async fn detect_rooms(
        &self,
        lines: &[Line],
        image_bytes: Option<&[u8]>,
    ) -> anyhow::Result<DetectionResult> {
        let start = Instant::now();
        let mut method_timings = Vec::new();

        match self.config.strategy {
            CombinationStrategy::GraphOnly => {
                self.detect_graph_only(lines, &mut method_timings).await
            }
            CombinationStrategy::GraphWithVision => {
                self.detect_graph_with_vision(lines, image_bytes, &mut method_timings)
                    .await
            }
            CombinationStrategy::YoloOnly => {
                self.detect_yolo_only(image_bytes, &mut method_timings)
                    .await
            }
            CombinationStrategy::BestAvailable => {
                self.detect_best_available(lines, image_bytes, &mut method_timings)
                    .await
            }
            CombinationStrategy::Ensemble => {
                self.detect_ensemble(lines, image_bytes, &mut method_timings)
                    .await
            }
        }
        .map(|mut result| {
            result.execution_time_ms = start.elapsed().as_millis();
            result.metadata.total_execution_time_ms = start.elapsed().as_millis();
            result.metadata.method_timings = method_timings;
            result
        })
    }

    /// Graph-based detection only
    async fn detect_graph_only(
        &self,
        lines: &[Line],
        timings: &mut Vec<(String, u128)>,
    ) -> anyhow::Result<DetectionResult> {
        let start = Instant::now();

        // Build graph and detect rooms
        let graph = crate::graph_builder::build_graph_with_door_threshold(
            lines,
            self.config.door_threshold,
        );

        let rooms = crate::room_detector::detect_rooms(
            &graph,
            self.config.area_threshold,
        );

        let elapsed = start.elapsed().as_millis();
        timings.push(("graph_detection".to_string(), elapsed));

        info!("Graph detection found {} rooms in {}ms", rooms.len(), elapsed);

        let enhanced_rooms: Vec<EnhancedRoom> = rooms
            .into_iter()
            .map(|room| EnhancedRoom {
                room,
                room_type: None,
                confidence: None,
                features: Vec::new(),
                detection_method: "graph".to_string(),
            })
            .collect();

        Ok(DetectionResult {
            rooms: enhanced_rooms.clone(),
            method_used: "graph_only".to_string(),
            execution_time_ms: elapsed,
            metadata: DetectionMetadata {
                graph_based_rooms: enhanced_rooms.len(),
                vision_classified: 0,
                yolo_detected: 0,
                total_execution_time_ms: elapsed,
                method_timings: timings.clone(),
            },
        })
    }

    /// Graph detection + Vision classification
    async fn detect_graph_with_vision(
        &self,
        lines: &[Line],
        image_bytes: Option<&[u8]>,
        timings: &mut Vec<(String, u128)>,
    ) -> anyhow::Result<DetectionResult> {
        // First do graph detection
        let graph_result = self.detect_graph_only(lines, timings).await?;

        // If no image provided or vision disabled, return graph-only results
        if image_bytes.is_none() || !self.config.enable_vision {
            warn!("Vision classification requested but image not provided or vision disabled");
            return Ok(graph_result);
        }

        let image_bytes = image_bytes.unwrap();

        // Try to classify with vision
        let vision_start = Instant::now();

        match self.classify_with_vision(image_bytes, &graph_result.rooms).await {
            Ok(enhanced_rooms) => {
                let vision_elapsed = vision_start.elapsed().as_millis();
                timings.push(("vision_classification".to_string(), vision_elapsed));

                let vision_classified = enhanced_rooms
                    .iter()
                    .filter(|r| r.room_type.is_some())
                    .count();

                info!(
                    "Vision classification enhanced {}/{} rooms in {}ms",
                    vision_classified,
                    enhanced_rooms.len(),
                    vision_elapsed
                );

                Ok(DetectionResult {
                    rooms: enhanced_rooms.clone(),
                    method_used: "graph_with_vision".to_string(),
                    execution_time_ms: graph_result.execution_time_ms + vision_elapsed,
                    metadata: DetectionMetadata {
                        graph_based_rooms: enhanced_rooms.len(),
                        vision_classified,
                        yolo_detected: 0,
                        total_execution_time_ms: 0, // Will be set by caller
                        method_timings: timings.clone(),
                    },
                })
            }
            Err(e) => {
                warn!("Vision classification failed: {}", e);
                // Fallback to graph-only results
                Ok(graph_result)
            }
        }
    }

    /// YOLO detection only (when model is available)
    async fn detect_yolo_only(
        &self,
        image_bytes: Option<&[u8]>,
        timings: &mut Vec<(String, u128)>,
    ) -> anyhow::Result<DetectionResult> {
        if image_bytes.is_none() {
            return Err(anyhow::anyhow!("YOLO detection requires image data"));
        }

        let image_bytes = image_bytes.unwrap();
        let start = Instant::now();

        // Check if YOLO model is available
        if !unified_detector::yolo::is_yolo_available() {
            return Err(anyhow::anyhow!(
                "YOLO model not found. Train model first: yolo-training/train_yolov8.py"
            ));
        }

        // Create YOLO detector (stub for now, will be real when ONNX runtime is added)
        let config = unified_detector::yolo::YoloConfig::default();
        let detector = unified_detector::yolo::StubYoloDetector::new(config)?;

        // Run detection
        let detections = detector.detect(image_bytes)?;

        let elapsed = start.elapsed().as_millis();
        timings.push(("yolo_detection".to_string(), elapsed));

        info!("YOLO detection found {} rooms in {}ms", detections.len(), elapsed);

        // Convert YOLO detections to our Room format
        let rooms: Vec<Room> = detections
            .iter()
            .enumerate()
            .map(|(idx, det)| Room {
                id: idx,
                bounding_box: [
                    det.bbox[0] as f64,
                    det.bbox[1] as f64,
                    det.bbox[2] as f64,
                    det.bbox[3] as f64,
                ],
                area: ((det.bbox[2] - det.bbox[0]) * (det.bbox[3] - det.bbox[1])) as f64,
                name_hint: det.class_name.clone(),
                points: Vec::new(), // YOLO doesn't provide polygon points
            })
            .collect();

        // Wrap in EnhancedRoom
        let enhanced_rooms: Vec<EnhancedRoom> = rooms
            .into_iter()
            .zip(detections.iter())
            .map(|(room, det)| EnhancedRoom {
                room,
                room_type: Some(det.class_name.clone()),
                confidence: Some(det.confidence as f64),
                features: Vec::new(),
                detection_method: "yolo".to_string(),
            })
            .collect();

        Ok(DetectionResult {
            rooms: enhanced_rooms.clone(),
            method_used: "yolo_only".to_string(),
            execution_time_ms: elapsed,
            metadata: DetectionMetadata {
                graph_based_rooms: 0,
                vision_classified: 0,
                yolo_detected: enhanced_rooms.len(),
                total_execution_time_ms: elapsed,
                method_timings: timings.clone(),
            },
        })
    }

    /// Use best available method (fallback chain)
    async fn detect_best_available(
        &self,
        lines: &[Line],
        image_bytes: Option<&[u8]>,
        timings: &mut Vec<(String, u128)>,
    ) -> anyhow::Result<DetectionResult> {
        // Priority: YOLO > Graph+Vision > Graph-only

        if self.config.enable_yolo && image_bytes.is_some() {
            if let Ok(result) = self.detect_yolo_only(image_bytes, timings).await {
                return Ok(result);
            }
        }

        if self.config.enable_vision && image_bytes.is_some() {
            if let Ok(result) = self.detect_graph_with_vision(lines, image_bytes, timings).await {
                return Ok(result);
            }
        }

        // Fallback to graph-only
        self.detect_graph_only(lines, timings).await
    }

    /// Run all available methods and return comparison
    async fn detect_ensemble(
        &self,
        lines: &[Line],
        image_bytes: Option<&[u8]>,
        timings: &mut Vec<(String, u128)>,
    ) -> anyhow::Result<DetectionResult> {
        let mut results = Vec::new();

        // Always run graph detection
        if let Ok(graph_result) = self.detect_graph_only(lines, timings).await {
            results.push(graph_result);
        }

        // Run vision if enabled and image provided
        if self.config.enable_vision && image_bytes.is_some() {
            if let Ok(vision_result) = self.detect_graph_with_vision(lines, image_bytes, timings).await {
                results.push(vision_result);
            }
        }

        // Run YOLO if enabled and image provided
        if self.config.enable_yolo && image_bytes.is_some() {
            if let Ok(yolo_result) = self.detect_yolo_only(image_bytes, timings).await {
                results.push(yolo_result);
            }
        }

        if results.is_empty() {
            return Err(anyhow::anyhow!("All detection methods failed"));
        }

        // Return the result with highest confidence
        // For now, prefer vision-enhanced results
        let best = results
            .into_iter()
            .max_by_key(|r| r.metadata.vision_classified)
            .unwrap();

        Ok(best)
    }

    /// Classify rooms using vision API
    async fn classify_with_vision(
        &self,
        image_bytes: &[u8],
        geometric_rooms: &[EnhancedRoom],
    ) -> anyhow::Result<Vec<EnhancedRoom>> {
        // Check for OpenAI API key
        let api_key = std::env::var("OPENAI_API_KEY")
            .map_err(|_| anyhow::anyhow!("OPENAI_API_KEY not set"))?;

        // Create vision classifier
        let classifier = vision_classifier::VisionClassifier::new(api_key, Some("gpt-5".to_string()));

        // Convert to format expected by vision classifier
        let geo_rooms: Vec<vision_classifier::GeometricRoom> = geometric_rooms
            .iter()
            .map(|r| vision_classifier::GeometricRoom {
                id: r.room.id,
                bounding_box: r.room.bounding_box,
                area: r.room.area,
            })
            .collect();

        // Classify
        let enhanced = classifier
            .enhance_detections(image_bytes, &geo_rooms)
            .await
            .map_err(|e| anyhow::anyhow!("Vision classification failed: {}", e))?;

        // Merge with our room data
        let result: Vec<EnhancedRoom> = geometric_rooms
            .iter()
            .zip(enhanced.iter())
            .map(|(geo_room, enhanced_room)| EnhancedRoom {
                room: geo_room.room.clone(),
                room_type: enhanced_room
                    .classification
                    .as_ref()
                    .map(|c| c.room_type.clone()),
                confidence: enhanced_room
                    .classification
                    .as_ref()
                    .map(|c| c.confidence),
                features: enhanced_room
                    .classification
                    .as_ref()
                    .map(|c| c.features.clone())
                    .unwrap_or_default(),
                detection_method: "graph_with_vision".to_string(),
            })
            .collect();

        Ok(result)
    }
}
