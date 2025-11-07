use crate::{Line, Point};
use aws_sdk_textract::operation::analyze_document::AnalyzeDocumentOutput;

/// Parse Textract response and extract architectural lines
pub fn parse_textract_response(response: AnalyzeDocumentOutput) -> Result<Vec<Line>, String> {
    let mut lines = Vec::new();

    let blocks = response.blocks().ok_or("No blocks in response")?;

    for block in blocks {
        // Look for LINE blocks which represent detected lines
        if block.block_type().map(|t| t.as_str()) == Some("LINE") {
            if let Some(geometry) = block.geometry() {
                if let Some(bbox) = geometry.bounding_box() {
                    // Extract bounding box coordinates
                    let left = bbox.left().unwrap_or(0.0);
                    let top = bbox.top().unwrap_or(0.0);
                    let width = bbox.width().unwrap_or(0.0);
                    let height = bbox.height().unwrap_or(0.0);

                    // Determine if this is a horizontal or vertical line based on aspect ratio
                    let aspect_ratio = width / height.max(0.001);

                    if aspect_ratio > 3.0 {
                        // Horizontal line
                        lines.push(Line {
                            start: Point {
                                x: left * 1000.0,
                                y: (top + height / 2.0) * 1000.0,
                            },
                            end: Point {
                                x: (left + width) * 1000.0,
                                y: (top + height / 2.0) * 1000.0,
                            },
                        });
                    } else if aspect_ratio < 0.33 {
                        // Vertical line
                        lines.push(Line {
                            start: Point {
                                x: (left + width / 2.0) * 1000.0,
                                y: top * 1000.0,
                            },
                            end: Point {
                                x: (left + width / 2.0) * 1000.0,
                                y: (top + height) * 1000.0,
                            },
                        });
                    }
                }

                // Also extract from polygon points if available
                if let Some(polygon) = geometry.polygon() {
                    if polygon.len() >= 2 {
                        // Extract start and end points from polygon
                        let start_point = &polygon[0];
                        let end_point = &polygon[polygon.len() - 1];

                        if let (Some(sx), Some(sy), Some(ex), Some(ey)) = (
                            start_point.x(),
                            start_point.y(),
                            end_point.x(),
                            end_point.y(),
                        ) {
                            lines.push(Line {
                                start: Point {
                                    x: sx * 1000.0,
                                    y: sy * 1000.0,
                                },
                                end: Point {
                                    x: ex * 1000.0,
                                    y: ey * 1000.0,
                                },
                            });
                        }
                    }
                }
            }
        }

        // Also look for LAYOUT_FIGURE blocks which might contain architectural elements
        if block.block_type().map(|t| t.as_str()) == Some("LAYOUT_FIGURE") {
            if let Some(geometry) = block.geometry() {
                if let Some(bbox) = geometry.bounding_box() {
                    let left = bbox.left().unwrap_or(0.0);
                    let top = bbox.top().unwrap_or(0.0);
                    let width = bbox.width().unwrap_or(0.0);
                    let height = bbox.height().unwrap_or(0.0);

                    // Extract edges of the figure as potential walls
                    // Top edge
                    lines.push(Line {
                        start: Point {
                            x: left * 1000.0,
                            y: top * 1000.0,
                        },
                        end: Point {
                            x: (left + width) * 1000.0,
                            y: top * 1000.0,
                        },
                    });

                    // Right edge
                    lines.push(Line {
                        start: Point {
                            x: (left + width) * 1000.0,
                            y: top * 1000.0,
                        },
                        end: Point {
                            x: (left + width) * 1000.0,
                            y: (top + height) * 1000.0,
                        },
                    });

                    // Bottom edge
                    lines.push(Line {
                        start: Point {
                            x: (left + width) * 1000.0,
                            y: (top + height) * 1000.0,
                        },
                        end: Point {
                            x: left * 1000.0,
                            y: (top + height) * 1000.0,
                        },
                    });

                    // Left edge
                    lines.push(Line {
                        start: Point {
                            x: left * 1000.0,
                            y: (top + height) * 1000.0,
                        },
                        end: Point {
                            x: left * 1000.0,
                            y: top * 1000.0,
                        },
                    });
                }
            }
        }
    }

    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_line_creation() {
        let line = Line {
            start: Point { x: 0.0, y: 0.0 },
            end: Point { x: 100.0, y: 0.0 },
        };

        assert_eq!(line.start.x, 0.0);
        assert_eq!(line.end.x, 100.0);
    }
}
