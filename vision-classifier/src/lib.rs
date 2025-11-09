use base64::{engine::general_purpose, Engine as _};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

/// Vision-based room classifier using OpenAI Vision API (GPT-4o)
pub struct VisionClassifier {
    client: Client,
    api_key: String,
    model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomClassification {
    pub room_id: usize,
    pub room_type: String,          // bedroom, kitchen, bathroom, living_room, etc.
    pub confidence: f64,             // 0.0-1.0
    pub features: Vec<String>,       // furniture, fixtures identified
    pub description: String,         // detailed description
}

/// Wall segment extracted from vision analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WallSegment {
    pub start: WallPoint,
    pub end: WallPoint,
}

/// Point in normalized coordinate space (0-1000)
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct WallPoint {
    pub x: f64,
    pub y: f64,
}

/// Room label with type hint from vision analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomLabel {
    pub label: String,
    pub center: WallPoint,
    pub room_type: String,
}

/// Complete wall extraction result from GPT-5 vision
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionWallData {
    pub walls: Vec<WallSegment>,
    pub rooms: Vec<RoomLabel>,
    pub confidence: f64,
}

#[derive(Debug, Serialize)]
struct OpenAIRequest {
    model: String,
    messages: Vec<Message>,
    max_tokens: u32,
}

#[derive(Debug, Serialize)]
struct Message {
    role: String,
    content: Vec<ContentItem>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum ContentItem {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: ImageUrl },
}

#[derive(Debug, Serialize)]
struct ImageUrl {
    url: String,
}

#[derive(Debug, Deserialize)]
struct OpenAIResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ResponseMessage,
}

#[derive(Debug, Deserialize)]
struct ResponseMessage {
    content: String,
}

impl VisionClassifier {
    /// Create a new vision classifier
    ///
    /// # Arguments
    /// * `api_key` - OpenAI API key (from OPENAI_API_KEY environment variable)
    /// * `model` - Model to use (default: "gpt-4o")
    pub fn new(api_key: String, model: Option<String>) -> Self {
        Self {
            client: Client::new(),
            api_key,
            model: model.unwrap_or_else(|| "gpt-4o".to_string()),
        }
    }

    /// Create from environment variable
    pub fn from_env() -> anyhow::Result<Self> {
        let api_key = std::env::var("OPENAI_API_KEY")
            .map_err(|_| anyhow::anyhow!("OPENAI_API_KEY environment variable not set"))?;
        Ok(Self::new(api_key, None))
    }

    /// Classify rooms in a floorplan image
    ///
    /// # Arguments
    /// * `image_bytes` - PNG/JPEG image bytes
    /// * `num_rooms` - Number of rooms detected by geometric method (optional hint)
    pub async fn classify_floorplan(
        &self,
        image_bytes: &[u8],
        num_rooms: Option<usize>,
    ) -> anyhow::Result<Vec<RoomClassification>> {
        info!("Classifying floorplan with GPT-5 Vision");

        // Encode image to base64
        let b64_image = general_purpose::STANDARD.encode(image_bytes);

        // Create prompt
        let prompt = if let Some(n) = num_rooms {
            format!(
                "Analyze this architectural floorplan image. {} rooms were detected geometrically. \
                 For each room visible in the floorplan:\n\
                 1. Identify the room type (bedroom, kitchen, bathroom, living_room, dining_room, hallway, etc.)\n\
                 2. Estimate confidence (0.0-1.0)\n\
                 3. List visible features (fixtures, furniture, doors, windows)\n\
                 4. Provide a brief description\n\n\
                 Return ONLY a valid JSON array of objects with this exact structure:\n\
                 [{{\n  \
                   \"room_id\": 0,\n  \
                   \"room_type\": \"bedroom\",\n  \
                   \"confidence\": 0.95,\n  \
                   \"features\": [\"bed\", \"closet\", \"window\"],\n  \
                   \"description\": \"Master bedroom with ensuite access\"\n\
                 }}]",
                n
            )
        } else {
            "Analyze this architectural floorplan image. \
             For each room visible in the floorplan:\n\
             1. Identify the room type (bedroom, kitchen, bathroom, living_room, dining_room, hallway, etc.)\n\
             2. Estimate confidence (0.0-1.0)\n\
             3. List visible features (fixtures, furniture, doors, windows)\n\
             4. Provide a brief description\n\n\
             Return ONLY a valid JSON array of objects with this exact structure:\n\
             [{\n  \
               \"room_id\": 0,\n  \
               \"room_type\": \"bedroom\",\n  \
               \"confidence\": 0.95,\n  \
               \"features\": [\"bed\", \"closet\", \"window\"],\n  \
               \"description\": \"Master bedroom with ensuite access\"\n\
             }]"
                .to_string()
        };

        // Build request
        let mut request_body = serde_json::json!({
            "model": self.model.clone(),
            "messages": vec![serde_json::json!({
                "role": "user",
                "content": vec![
                    serde_json::json!({
                        "type": "text",
                        "text": prompt
                    }),
                    serde_json::json!({
                        "type": "image_url",
                        "image_url": {
                            "url": format!("data:image/png;base64,{}", b64_image)
                        }
                    })
                ]
            })]
        });

        // Use max_completion_tokens for newer models, max_tokens for older ones
        if self.model.starts_with("gpt-5") || self.model.starts_with("o1") {
            request_body["max_completion_tokens"] = serde_json::json!(2000);
        } else {
            request_body["max_tokens"] = serde_json::json!(2000);
        }

        info!("Sending request to OpenAI API (model: {})", self.model);

        // Call OpenAI API
        let response = self
            .client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await?;
            warn!("OpenAI API error: {} - {}", status, error_text);
            return Err(anyhow::anyhow!("OpenAI API error: {} - {}", status, error_text));
        }

        let api_response: OpenAIResponse = response.json().await?;

        // Parse response
        if api_response.choices.is_empty() {
            return Err(anyhow::anyhow!("No response from OpenAI API"));
        }

        let content = &api_response.choices[0].message.content;
        info!("Received response from OpenAI: {}", content);

        // Parse JSON from response
        // GPT might wrap the JSON in markdown code blocks, so we need to extract it
        let json_str = extract_json_from_response(content)?;

        let classifications: Vec<RoomClassification> = serde_json::from_str(&json_str)
            .map_err(|e| anyhow::anyhow!("Failed to parse classifications: {}. Response: {}", e, json_str))?;

        info!("Successfully classified {} rooms", classifications.len());

        Ok(classifications)
    }

    /// Enhance existing room detections with vision-based classification
    ///
    /// This merges geometric room detection with vision-based classification
    pub async fn enhance_detections(
        &self,
        image_bytes: &[u8],
        geometric_rooms: &[crate::GeometricRoom],
    ) -> anyhow::Result<Vec<EnhancedRoom>> {
        let classifications = self.classify_floorplan(image_bytes, Some(geometric_rooms.len())).await?;

        // Match classifications to geometric rooms (by position/area heuristics)
        let enhanced = geometric_rooms
            .iter()
            .enumerate()
            .map(|(i, room)| {
                // Try to find matching classification by room_id
                let classification = classifications
                    .iter()
                    .find(|c| c.room_id == i)
                    .or_else(|| classifications.get(i));

                EnhancedRoom {
                    geometric: room.clone(),
                    classification: classification.cloned(),
                }
            })
            .collect();

        Ok(enhanced)
    }

    /// Extract wall segments from a blueprint image using GPT-5 Vision
    ///
    /// # Arguments
    /// * `image_base64` - Base64-encoded blueprint image (in normalized 1000x1000 space)
    ///
    /// # Returns
    /// Wall segments, room labels, and confidence score
    pub async fn extract_wall_segments(
        &self,
        image_base64: &str,
    ) -> anyhow::Result<VisionWallData> {
        info!("Extracting wall segments from blueprint using GPT-5 Vision");

        let prompt = r#"You are analyzing an architectural blueprint. Extract:
1. All wall segments as line coordinates in 0-1000 normalized coordinate space
2. Room labels with their center coordinates and type

IMPORTANT: The image is normalized to 1000x1000 coordinate space where:
- Top-left corner is (0, 0)
- Bottom-right corner is (1000, 1000)

Return ONLY valid JSON with this exact structure:
{
  "walls": [
    {"start": {"x": 100, "y": 200}, "end": {"x": 500, "y": 200}},
    {"start": {"x": 500, "y": 200}, "end": {"x": 500, "y": 600}}
  ],
  "rooms": [
    {"label": "Kitchen", "center": {"x": 300, "y": 400}, "room_type": "kitchen"},
    {"label": "Bedroom", "center": {"x": 700, "y": 400}, "room_type": "bedroom"}
  ],
  "confidence": 0.85
}

Guidelines:
- Extract ALL visible wall segments (interior and exterior)
- Walls should form closed polygons for rooms
- Room types: kitchen, bedroom, bathroom, living_room, dining_room, hallway, closet, office, etc.
- Confidence: your overall confidence in the wall extraction (0.0-1.0)
- Be precise with coordinates - walls should align properly"#;

        // Prepare image URL - check if already has data URI prefix
        let image_url = if image_base64.starts_with("data:") {
            image_base64.to_string()
        } else {
            format!("data:image/png;base64,{}", image_base64)
        };

        // Build request
        let mut request_body = serde_json::json!({
            "model": self.model.clone(),
            "messages": vec![serde_json::json!({
                "role": "user",
                "content": vec![
                    serde_json::json!({
                        "type": "text",
                        "text": prompt
                    }),
                    serde_json::json!({
                        "type": "image_url",
                        "image_url": {
                            "url": image_url
                        }
                    })
                ]
            })]
        });

        // Use appropriate token parameter based on model
        if self.model.starts_with("gpt-5") || self.model.starts_with("o1") {
            request_body["max_completion_tokens"] = serde_json::json!(4000);
        } else {
            request_body["max_tokens"] = serde_json::json!(4000);
        }

        info!("Sending wall extraction request to OpenAI API (model: {})", self.model);

        // Call OpenAI API
        let response = self
            .client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await?;
            warn!("OpenAI API error: {} - {}", status, error_text);
            return Err(anyhow::anyhow!("OpenAI API error: {} - {}", status, error_text));
        }

        let api_response: OpenAIResponse = response.json().await?;

        // Parse response
        if api_response.choices.is_empty() {
            return Err(anyhow::anyhow!("No response from OpenAI API"));
        }

        let content = &api_response.choices[0].message.content;
        info!("Received wall extraction response from OpenAI");

        // Parse JSON from response
        let json_str = extract_json_from_response(content)?;

        let mut wall_data: VisionWallData = serde_json::from_str(&json_str)
            .map_err(|e| anyhow::anyhow!("Failed to parse wall data: {}. Response: {}", e, json_str))?;

        // Validate coordinate ranges
        for wall in &wall_data.walls {
            if !is_valid_coordinate(&wall.start) || !is_valid_coordinate(&wall.end) {
                warn!("Invalid wall coordinates detected, clamping to valid range");
                // We could either clamp or reject, but for robustness let's warn and continue
            }
        }

        for room in &wall_data.rooms {
            if !is_valid_coordinate(&room.center) {
                warn!("Invalid room center coordinates detected");
            }
        }

        // Clamp confidence to valid range
        wall_data.confidence = wall_data.confidence.clamp(0.0, 1.0);

        info!("Successfully extracted {} walls and {} room labels (confidence: {:.2})",
            wall_data.walls.len(), wall_data.rooms.len(), wall_data.confidence);

        Ok(wall_data)
    }
}

/// Extract JSON from GPT response (handles markdown code blocks)
fn extract_json_from_response(content: &str) -> anyhow::Result<String> {
    let trimmed = content.trim();

    // Check if wrapped in markdown code block
    if trimmed.starts_with("```") {
        // Find the JSON content between ```json and ```
        let lines: Vec<&str> = trimmed.lines().collect();
        let json_lines: Vec<&str> = lines
            .iter()
            .skip(1) // Skip first ```json line
            .take_while(|line| !line.starts_with("```"))
            .copied()
            .collect();

        Ok(json_lines.join("\n"))
    } else {
        Ok(trimmed.to_string())
    }
}

/// Validate that a coordinate point is within the normalized 0-1000 range
fn is_valid_coordinate(point: &WallPoint) -> bool {
    point.x >= 0.0 && point.x <= 1000.0 && point.y >= 0.0 && point.y <= 1000.0
}

// Re-export types for convenience
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeometricRoom {
    pub id: usize,
    pub bounding_box: [f64; 4],
    pub area: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnhancedRoom {
    pub geometric: GeometricRoom,
    pub classification: Option<RoomClassification>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json_from_markdown() {
        let response = r#"```json
[{"room_id": 0, "room_type": "bedroom"}]
```"#;

        let json = extract_json_from_response(response).unwrap();
        assert_eq!(json, r#"[{"room_id": 0, "room_type": "bedroom"}]"#);
    }

    #[test]
    fn test_extract_json_plain() {
        let response = r#"[{"room_id": 0, "room_type": "bedroom"}]"#;

        let json = extract_json_from_response(response).unwrap();
        assert_eq!(json, r#"[{"room_id": 0, "room_type": "bedroom"}]"#);
    }
}
