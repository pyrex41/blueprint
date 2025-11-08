# Default recipe to display help information
default:
    @just --list

# Build the entire workspace
build:
    cargo build --workspace

# Build release version
build-release:
    cargo build --workspace --release

# Build frontend WASM
build-frontend:
    cd leptos-frontend && trunk build

# Build frontend for release
build-frontend-release:
    cd leptos-frontend && trunk build --release

# Run backend server
run-backend:
    cargo run --bin axum-backend

# Run validation pipeline
run-validation:
    cargo run --bin validation-pipeline

# Check all crates
check:
    cargo check --workspace

# Run tests across workspace
test:
    cargo test --workspace

# Run benchmarks
bench:
    cargo bench --workspace

# Lint with biome
lint:
    biome check .

# Format with biome
format:
    biome format --write .

# Clean build artifacts
clean:
    cargo clean

# Check dependencies for updates
deps-update:
    cargo update

# Build tauri app
build-tauri:
    cd tauri-stretch && cargo tauri build

# Run tauri in dev mode
run-tauri:
    cd tauri-stretch && cargo tauri dev
