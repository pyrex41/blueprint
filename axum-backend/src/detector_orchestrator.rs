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
    /// Confidence threshold for hybrid vision merge strategy (0.0-1.0)
    #[serde(default = "default_confidence_threshold")]
    pub confidence_threshold: f64,
}

fn default_confidence_threshold() -> f64 {
    0.75
}

impl Default for DetectorConfig {
    fn default() -> Self {
        Self {
            area_threshold: 100.0,
            door_threshold: 50.0,
            enable_vision: false, // Disabled by default (requires API key)
            enable_yolo: false,   // Disabled until model is trained
            strategy: CombinationStrategy::GraphOnly,
            confidence_threshold: 0.75,
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
    /// Hybrid vision: VTracer + GPT-5 Vision merged wall extraction
    HybridVision,
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
    // Hybrid vision specific metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vtracer_walls_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpt5_walls_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merged_walls_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consensus_walls_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpt5_confidence: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merge_strategy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merged_walls: Option<Vec<crate::wall_merger::Line>>,
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
            CombinationStrategy::HybridVision => {
                self.detect_hybrid_vision(image_bytes, &mut method_timings)
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
            1.5, // Default outer boundary ratio
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
                vtracer_walls_count: None,
                gpt5_walls_count: None,
                merged_walls_count: None,
                consensus_walls_count: None,
                gpt5_confidence: None,
                merge_strategy: None,
                merged_walls: None,
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
                        vtracer_walls_count: None,
                        gpt5_walls_count: None,
                        merged_walls_count: None,
                        consensus_walls_count: None,
                        gpt5_confidence: None,
                        merge_strategy: None,
                        merged_walls: None,
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
                vtracer_walls_count: None,
                gpt5_walls_count: None,
                merged_walls_count: None,
                consensus_walls_count: None,
                gpt5_confidence: None,
                merge_strategy: None,
                merged_walls: None,
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

    /// Hybrid vision detection: VTracer + GPT-5 Vision wall extraction
    async fn detect_hybrid_vision(
        &self,
        image_bytes: Option<&[u8]>,
        timings: &mut Vec<(String, u128)>,
    ) -> anyhow::Result<DetectionResult> {
        if image_bytes.is_none() {
            return Err(anyhow::anyhow!("Hybrid vision detection requires image data"));
        }

        let image_bytes = image_bytes.unwrap();

        // Step 0: Check API key first (fail fast before expensive operations)
        std::env::var("OPENAI_API_KEY")
            .map_err(|_| anyhow::anyhow!("OPENAI_API_KEY not set"))?;

        // Step 1: Normalize image
        let norm_start = Instant::now();
        let normalized_image = crate::image_preprocessor::NormalizedImage::from_bytes(image_bytes)
            .map_err(|e| anyhow::anyhow!("Image normalization failed: {}", e))?;

        let norm_elapsed = norm_start.elapsed().as_millis();
        timings.push(("image_normalization".to_string(), norm_elapsed));
        info!("Image normalized in {}ms", norm_elapsed);

        // Step 2: Run VTracer and GPT-5 Vision in parallel
        let vtracer_start = Instant::now();
        let gpt5_start = Instant::now();

        // VTracer vectorization
        let vtracer_future = async {
            let vtracer_start = Instant::now();

            // VTracer requires file paths, use UUID for unique temp file names (prevents race conditions)
            let request_id = uuid::Uuid::new_v4();
            let temp_path = std::env::temp_dir().join(format!("hybrid_{}_input.png", request_id));
            let svg_path = std::env::temp_dir().join(format!("hybrid_{}_output.svg", request_id));

            // Save image to temp file
            std::fs::write(&temp_path, image_bytes)?;

            // Configure VTracer
            let config = vtracer::Config {
                color_mode: vtracer::ColorMode::Binary,
                hierarchical: vtracer::Hierarchical::Stacked,
                mode: visioncortex::PathSimplifyMode::Spline,
                filter_speckle: 4,
                color_precision: 6,
                layer_difference: 16,
                corner_threshold: 60,
                length_threshold: 4.0,
                max_iterations: 10,
                splice_threshold: 45,
                path_precision: Some(3),
            };

            // Convert to SVG
            let vtracer_result = vtracer::convert_image_to_svg(&temp_path, &svg_path, config)
                .map_err(|e| anyhow::anyhow!("VTracer failed: {}", e));

            // Read SVG (only if vtracer succeeded)
            let svg_result = vtracer_result.and_then(|_| {
                std::fs::read_to_string(&svg_path)
                    .map_err(|e| anyhow::anyhow!("Failed to read SVG: {}", e))
            });

            // Always clean up temp files, logging failures
            if let Err(e) = std::fs::remove_file(&temp_path) {
                warn!("Failed to remove temp input file {}: {}", temp_path.display(), e);
            }
            if let Err(e) = std::fs::remove_file(&svg_path) {
                warn!("Failed to remove temp SVG file {}: {}", svg_path.display(), e);
            }

            // Return SVG processing result
            let svg = svg_result?;

            let vectorizer_lines = crate::image_vectorizer::parse_svg_to_lines(&svg)?;

            // Convert image_vectorizer::Line to crate::Line
            let lines: Vec<Line> = vectorizer_lines.iter().map(|vl| Line {
                start: crate::Point { x: vl.start.x, y: vl.start.y },
                end: crate::Point { x: vl.end.x, y: vl.end.y },
                is_load_bearing: vl.is_load_bearing,
            }).collect();

            let elapsed = vtracer_start.elapsed().as_millis();
            Ok::<(Vec<Line>, u128), anyhow::Error>((lines, elapsed))
        };

        // GPT-5 Vision wall extraction
        let gpt5_future = async {
            let gpt5_start = Instant::now();

            // Check for OpenAI API key
            let api_key = std::env::var("OPENAI_API_KEY")
                .map_err(|_| anyhow::anyhow!("OPENAI_API_KEY not set"))?;

            let classifier = vision_classifier::VisionClassifier::new(api_key, Some("gpt-4o".to_string()));

            let wall_data = classifier
                .extract_wall_segments(&normalized_image.base64_data)
                .await?;

            let elapsed = gpt5_start.elapsed().as_millis();
            Ok::<(vision_classifier::VisionWallData, u128), anyhow::Error>((wall_data, elapsed))
        };

        // Run both in parallel
        let (vtracer_result, gpt5_result) = tokio::join!(vtracer_future, gpt5_future);

        let (vtracer_lines, vtracer_elapsed) = vtracer_result?;
        let (vision_data, gpt5_elapsed) = gpt5_result?;

        timings.push(("vtracer_vectorization".to_string(), vtracer_elapsed));
        timings.push(("gpt5_wall_extraction".to_string(), gpt5_elapsed));

        info!(
            "VTracer found {} walls in {}ms, GPT-5 found {} walls in {}ms",
            vtracer_lines.len(),
            vtracer_elapsed,
            vision_data.walls.len(),
            gpt5_elapsed
        );

        // Step 3: Convert GPT-5 walls to Line format (denormalize coordinates)
        let vision_lines: Vec<Line> = vision_data
            .walls
            .iter()
            .map(|wall| {
                let start_orig = normalized_image.denormalize_point(
                    crate::image_preprocessor::NormalizedPoint {
                        x: wall.start.x,
                        y: wall.start.y,
                    },
                );
                let end_orig = normalized_image.denormalize_point(
                    crate::image_preprocessor::NormalizedPoint {
                        x: wall.end.x,
                        y: wall.end.y,
                    },
                );

                Line {
                    start: crate::Point {
                        x: start_orig.x,
                        y: start_orig.y,
                    },
                    end: crate::Point {
                        x: end_orig.x,
                        y: end_orig.y,
                    },
                    is_load_bearing: false,
                }
            })
            .collect();

        // Step 4: Merge wall segments using confidence-based strategy
        let merge_start = Instant::now();

        // Convert to wall_merger format
        let vtracer_walls_for_merge: Vec<crate::wall_merger::Line> = vtracer_lines
            .iter()
            .map(|l| crate::wall_merger::Line {
                start: crate::wall_merger::Point { x: l.start.x, y: l.start.y },
                end: crate::wall_merger::Point { x: l.end.x, y: l.end.y },
                is_load_bearing: l.is_load_bearing,
                source: None,
            })
            .collect();

        let vision_walls_for_merge: Vec<crate::wall_merger::Line> = vision_lines
            .iter()
            .map(|l| crate::wall_merger::Line {
                start: crate::wall_merger::Point { x: l.start.x, y: l.start.y },
                end: crate::wall_merger::Point { x: l.end.x, y: l.end.y },
                is_load_bearing: l.is_load_bearing,
                source: None,
            })
            .collect();

        let merge_result = crate::wall_merger::merge_wall_segments(
            vtracer_walls_for_merge,
            vision_walls_for_merge,
            vision_data.confidence,
            self.config.confidence_threshold,
        );

        let merge_elapsed = merge_start.elapsed().as_millis();
        timings.push(("wall_merging".to_string(), merge_elapsed));

        info!(
            "Merged {} walls using {} strategy ({} consensus) in {}ms",
            merge_result.metadata.merged_count,
            merge_result.metadata.strategy_used,
            merge_result.metadata.consensus_count,
            merge_elapsed
        );

        // Step 5: Convert merged walls back to our Line format
        let merged_lines: Vec<Line> = merge_result
            .walls
            .iter()
            .map(|wall| Line {
                start: crate::Point {
                    x: wall.start.x,
                    y: wall.start.y,
                },
                end: crate::Point {
                    x: wall.end.x,
                    y: wall.end.y,
                },
                is_load_bearing: wall.is_load_bearing,
            })
            .collect();

        // Step 6: Detect rooms from merged walls
        let detection_start = Instant::now();

        let graph = crate::graph_builder::build_graph_with_door_threshold(
            &merged_lines,
            self.config.door_threshold,
        );

        let rooms = crate::room_detector::detect_rooms(
            &graph,
            self.config.area_threshold,
            1.5, // Default outer boundary ratio
        );

        let detection_elapsed = detection_start.elapsed().as_millis();
        timings.push(("room_detection".to_string(), detection_elapsed));

        info!("Detected {} rooms in {}ms", rooms.len(), detection_elapsed);

        // Step 7: Enhance rooms with GPT-5 room labels
        let mut enhanced_rooms: Vec<EnhancedRoom> = rooms
            .into_iter()
            .map(|room| {
                // Try to match with GPT-5 room labels based on bounding box center
                let room_center_x = (room.bounding_box[0] + room.bounding_box[2]) / 2.0;
                let room_center_y = (room.bounding_box[1] + room.bounding_box[3]) / 2.0;

                // Find closest room label
                let closest_label = vision_data
                    .rooms
                    .iter()
                    .min_by_key(|label| {
                        let dx = label.center.x - room_center_x;
                        let dy = label.center.y - room_center_y;
                        ((dx * dx + dy * dy).sqrt() * 1000.0) as i64
                    });

                EnhancedRoom {
                    room,
                    room_type: closest_label.map(|l| l.room_type.clone()),
                    confidence: if closest_label.is_some() {
                        Some(vision_data.confidence)
                    } else {
                        None
                    },
                    features: Vec::new(),
                    detection_method: "hybrid_vision".to_string(),
                }
            })
            .collect();

        Ok(DetectionResult {
            rooms: enhanced_rooms.clone(),
            method_used: "hybrid_vision".to_string(),
            execution_time_ms: 0, // Will be set by caller
            metadata: DetectionMetadata {
                graph_based_rooms: enhanced_rooms.len(),
                vision_classified: enhanced_rooms
                    .iter()
                    .filter(|r| r.room_type.is_some())
                    .count(),
                yolo_detected: 0,
                total_execution_time_ms: 0, // Will be set by caller
                method_timings: timings.clone(),
                vtracer_walls_count: Some(merge_result.metadata.vtracer_count),
                gpt5_walls_count: Some(merge_result.metadata.gpt5_count),
                merged_walls_count: Some(merge_result.metadata.merged_count),
                consensus_walls_count: Some(merge_result.metadata.consensus_count),
                gpt5_confidence: Some(vision_data.confidence),
                merge_strategy: Some(merge_result.metadata.strategy_used.clone()),
                merged_walls: Some(merge_result.walls),
            },
        })
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
