use crate::{Line, Point, Room};
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

/// Render the floorplan with lines and detected rooms
pub fn render_floorplan(canvas: &HtmlCanvasElement, lines: &[Line], rooms: &[Room]) {
    let context = canvas
        .get_context("2d")
        .unwrap()
        .unwrap()
        .dyn_into::<CanvasRenderingContext2d>()
        .unwrap();

    let width = canvas.width() as f64;
    let height = canvas.height() as f64;

    // Clear canvas
    context.clear_rect(0.0, 0.0, width, height);

    // Fill with white background
    context.set_fill_style(&"white".into());
    context.fill_rect(0.0, 0.0, width, height);

    // Calculate bounds for scaling from lines or rooms
    let bounds = if !lines.is_empty() {
        calculate_bounds_from_lines(lines)
    } else if !rooms.is_empty() {
        calculate_bounds_from_rooms(rooms)
    } else {
        // Show placeholder text if no data
        context.set_fill_style(&"#999".into());
        context.set_font("20px sans-serif");
        context
            .fill_text("Upload a JSON file to see the floorplan", 200.0, height / 2.0)
            .unwrap();
        return;
    };

    // Calculate scale to fit canvas with padding
    let padding = 50.0;
    let scale_x = (width - 2.0 * padding) / (bounds.max_x - bounds.min_x);
    let scale_y = (height - 2.0 * padding) / (bounds.max_y - bounds.min_y);
    let scale = scale_x.min(scale_y);

    // Draw rooms first (as filled shapes)
    for room in rooms {
        context.set_fill_style(&room_color(room.id).into());
        context.set_global_alpha(0.3);
        context.begin_path();

        if !room.points.is_empty() {
            // Use polygon points if available
            let first = transform_point(&room.points[0], &bounds, scale, width, height, padding);
            context.move_to(first.0, first.1);

            for point in &room.points[1..] {
                let (x, y) = transform_point(point, &bounds, scale, width, height, padding);
                context.line_to(x, y);
            }

            context.close_path();
            context.fill();
        } else if room.bounding_box.len() >= 4 {
            // Fall back to bounding box if no points available
            let min_x = room.bounding_box[0];
            let min_y = room.bounding_box[1];
            let max_x = room.bounding_box[2];
            let max_y = room.bounding_box[3];

            let (x1, y1) = transform_point(&Point { x: min_x, y: min_y }, &bounds, scale, width, height, padding);
            let (x2, y2) = transform_point(&Point { x: max_x, y: max_y }, &bounds, scale, width, height, padding);

            context.fill_rect(x1, y2, x2 - x1, y1 - y2);
        }
        context.set_global_alpha(1.0);
    }

    // Draw lines (walls)
    context.set_stroke_style(&"#333".into());
    context.set_line_width(2.0);

    for line in lines {
        context.begin_path();

        let (start_x, start_y) =
            transform_point(&line.start, &bounds, scale, width, height, padding);
        let (end_x, end_y) = transform_point(&line.end, &bounds, scale, width, height, padding);

        context.move_to(start_x, start_y);
        context.line_to(end_x, end_y);
        context.stroke();
    }

    // Draw room labels
    context.set_fill_style(&"#000".into());
    context.set_font("14px sans-serif");

    for room in rooms {
        if room.bounding_box.len() >= 4 {
            // Calculate center of bounding box
            let center_x = (room.bounding_box[0] + room.bounding_box[2]) / 2.0;
            let center_y = (room.bounding_box[1] + room.bounding_box[3]) / 2.0;

            let center_point = Point {
                x: center_x,
                y: center_y,
            };

            let (label_x, label_y) =
                transform_point(&center_point, &bounds, scale, width, height, padding);

            let label = format!("Room {}", room.id);
            context.fill_text(&label, label_x - 20.0, label_y).unwrap();
        }
    }
}

/// Calculate the bounding box of all lines
fn calculate_bounds_from_lines(lines: &[Line]) -> Bounds {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for line in lines {
        min_x = min_x.min(line.start.x).min(line.end.x);
        min_y = min_y.min(line.start.y).min(line.end.y);
        max_x = max_x.max(line.start.x).max(line.end.x);
        max_y = max_y.max(line.start.y).max(line.end.y);
    }

    Bounds {
        min_x,
        min_y,
        max_x,
        max_y,
    }
}

/// Calculate the bounding box of all rooms
fn calculate_bounds_from_rooms(rooms: &[Room]) -> Bounds {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for room in rooms {
        if !room.points.is_empty() {
            // Use polygon points if available
            for point in &room.points {
                min_x = min_x.min(point.x);
                min_y = min_y.min(point.y);
                max_x = max_x.max(point.x);
                max_y = max_y.max(point.y);
            }
        } else if room.bounding_box.len() >= 4 {
            // Use bounding box if no points available
            min_x = min_x.min(room.bounding_box[0]);
            min_y = min_y.min(room.bounding_box[1]);
            max_x = max_x.max(room.bounding_box[2]);
            max_y = max_y.max(room.bounding_box[3]);
        }
    }

    Bounds {
        min_x,
        min_y,
        max_x,
        max_y,
    }
}

struct Bounds {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}

/// Transform a point from world coordinates to canvas coordinates
fn transform_point(
    point: &Point,
    bounds: &Bounds,
    scale: f64,
    _width: f64,
    height: f64,
    padding: f64,
) -> (f64, f64) {
    let x = (point.x - bounds.min_x) * scale + padding;
    let y = height - ((point.y - bounds.min_y) * scale + padding);
    (x, y)
}

/// Generate a color for a room based on its ID
fn room_color(id: usize) -> String {
    let colors = [
        "#FF6B6B", "#4ECDC4", "#45B7D1", "#FFA07A", "#98D8C8", "#F7DC6F", "#BB8FCE", "#85C1E2",
        "#F8B88B", "#ABEBC6",
    ];

    colors[id % colors.len()].to_string()
}
