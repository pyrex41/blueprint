#!/bin/bash
# Apply the best VTracer configuration to the backend

echo "Applying best VTracer configuration..."
echo ""
echo "Based on testing:"
echo "  - Config: 1_current_spline (current default)"
echo "  - Result: Extracted 4 walls from test_blueprint_001.png"
echo "  - This config already in use!"
echo ""
echo "Current settings are optimal. No changes needed."
echo ""
echo "Alternative promising configs to try:"
echo "  1. minimal_filter (filter_speckle=2) - Captured 112 paths (more detail)"
echo "  2. web_default (filter_speckle=37) - Cleaner output"
echo ""
echo "To experiment, edit: axum-backend/src/detector_orchestrator.rs:507-519"
