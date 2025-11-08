// Enhanced test program with door detection
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Line {
    pub start: Point,
    pub end: Point,
    #[serde(default)]
    pub is_load_bearing: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🏗️  Floorplan Room Detection Test Suite with Door Support");
    println!("===========================================================\n");

    // Test Case 1: Simple apartment with doors
    println!("📋 Test Case 1: Apartment with Doorways");
    let apartment_with_doors = create_apartment_with_doors();
    save_test("apartment_with_doors", apartment_with_doors, 50.0)?;

    // Test Case 2: Open floor plan
    println!("\n📋 Test Case 2: Open Floor Plan (large gaps)");
    let open_floor_plan = create_open_floor_plan();
    save_test("open_floor_plan", open_floor_plan, 150.0)?;

    // Test Case 3: Traditional closed rooms (no doors in data)
    println!("\n📋 Test Case 3: Closed Rooms (no gaps)");
    let closed_rooms = create_closed_rooms();
    save_test("closed_rooms", closed_rooms, 0.0)?;

    println!("\n✅ All test cases generated!");
    println!("\n🚀 To test:");
    println!("   1. Start server: cargo run --release --bin axum-backend");
    println!("   2. Test each case:");
    println!("      curl -X POST http://localhost:3000/detect \\");
    println!("           -H 'Content-Type: application/json' \\");
    println!("           -d @data/apartment_with_doors_request.json | jq");

    Ok(())
}

/// Create an apartment with door openings (gaps in walls)
fn create_apartment_with_doors() -> Vec<Line> {
    vec![
        // Living room (0,0 to 300,200) with door opening on right wall
        Line { start: Point { x: 0.0, y: 0.0 }, end: Point { x: 300.0, y: 0.0 }, is_load_bearing: true },

        // Right wall with gap for door (300,0 to 300,80) and (300,120 to 300,200)
        // Gap between y=80 and y=120 (40 units)
        Line { start: Point { x: 300.0, y: 0.0 }, end: Point { x: 300.0, y: 80.0 }, is_load_bearing: true },
        Line { start: Point { x: 300.0, y: 120.0 }, end: Point { x: 300.0, y: 200.0 }, is_load_bearing: true },

        Line { start: Point { x: 300.0, y: 200.0 }, end: Point { x: 0.0, y: 200.0 }, is_load_bearing: true },
        Line { start: Point { x: 0.0, y: 200.0 }, end: Point { x: 0.0, y: 0.0 }, is_load_bearing: true },

        // Bedroom (300,0 to 500,200) connected via the door gap
        Line { start: Point { x: 300.0, y: 0.0 }, end: Point { x: 500.0, y: 0.0 }, is_load_bearing: true },
        Line { start: Point { x: 500.0, y: 0.0 }, end: Point { x: 500.0, y: 200.0 }, is_load_bearing: true },
        Line { start: Point { x: 500.0, y: 200.0 }, end: Point { x: 300.0, y: 200.0 }, is_load_bearing: true },

        // Bathroom (0,200 to 150,350) with door gap
        // Door gap at x=0, between y=200 and y=240
        Line { start: Point { x: 0.0, y: 240.0 }, end: Point { x: 0.0, y: 350.0 }, is_load_bearing: false },
        Line { start: Point { x: 0.0, y: 350.0 }, end: Point { x: 150.0, y: 350.0 }, is_load_bearing: false },
        Line { start: Point { x: 150.0, y: 350.0 }, end: Point { x: 150.0, y: 200.0 }, is_load_bearing: false },
        Line { start: Point { x: 150.0, y: 200.0 }, end: Point { x: 0.0, y: 200.0 }, is_load_bearing: false },
    ]
}

/// Create an open floor plan with larger gaps
fn create_open_floor_plan() -> Vec<Line> {
    vec![
        // Main perimeter
        Line { start: Point { x: 0.0, y: 0.0 }, end: Point { x: 600.0, y: 0.0 }, is_load_bearing: true },
        Line { start: Point { x: 600.0, y: 0.0 }, end: Point { x: 600.0, y: 400.0 }, is_load_bearing: true },
        Line { start: Point { x: 600.0, y: 400.0 }, end: Point { x: 0.0, y: 400.0 }, is_load_bearing: true },
        Line { start: Point { x: 0.0, y: 400.0 }, end: Point { x: 0.0, y: 0.0 }, is_load_bearing: true },

        // Partial wall creating semi-open kitchen (100 unit gap)
        // Left side: x=200, y=0 to y=150
        Line { start: Point { x: 200.0, y: 0.0 }, end: Point { x: 200.0, y: 150.0 }, is_load_bearing: false },
        // Right side: x=200, y=250 to y=400 (gap from y=150 to y=250)
        Line { start: Point { x: 200.0, y: 250.0 }, end: Point { x: 200.0, y: 400.0 }, is_load_bearing: false },

        // Island counter (not connected to walls)
        Line { start: Point { x: 250.0, y: 200.0 }, end: Point { x: 350.0, y: 200.0 }, is_load_bearing: false },
        Line { start: Point { x: 350.0, y: 200.0 }, end: Point { x: 350.0, y: 250.0 }, is_load_bearing: false },
        Line { start: Point { x: 350.0, y: 250.0 }, end: Point { x: 250.0, y: 250.0 }, is_load_bearing: false },
        Line { start: Point { x: 250.0, y: 250.0 }, end: Point { x: 250.0, y: 200.0 }, is_load_bearing: false },
    ]
}

/// Create traditional closed rooms without gaps
fn create_closed_rooms() -> Vec<Line> {
    vec![
        // Room 1: 0,0 to 200,200
        Line { start: Point { x: 0.0, y: 0.0 }, end: Point { x: 200.0, y: 0.0 }, is_load_bearing: true },
        Line { start: Point { x: 200.0, y: 0.0 }, end: Point { x: 200.0, y: 200.0 }, is_load_bearing: true },
        Line { start: Point { x: 200.0, y: 200.0 }, end: Point { x: 0.0, y: 200.0 }, is_load_bearing: true },
        Line { start: Point { x: 0.0, y: 200.0 }, end: Point { x: 0.0, y: 0.0 }, is_load_bearing: true },

        // Room 2: 200,0 to 400,200
        Line { start: Point { x: 200.0, y: 0.0 }, end: Point { x: 400.0, y: 0.0 }, is_load_bearing: true },
        Line { start: Point { x: 400.0, y: 0.0 }, end: Point { x: 400.0, y: 200.0 }, is_load_bearing: true },
        Line { start: Point { x: 400.0, y: 200.0 }, end: Point { x: 200.0, y: 200.0 }, is_load_bearing: true },
    ]
}

fn save_test(name: &str, lines: Vec<Line>, door_threshold: f64) -> Result<(), Box<dyn std::error::Error>> {
    let request = serde_json::json!({
        "lines": lines,
        "area_threshold": 1000.0,
        "door_threshold": door_threshold
    });

    let filename = format!("data/{}_request.json", name);
    fs::write(&filename, serde_json::to_string_pretty(&request)?)?;

    println!("   ✅ Saved: {}", filename);
    println!("   📊 Lines: {}", lines.len());
    println!("   🚪 Door threshold: {} units", door_threshold);

    Ok(())
}
