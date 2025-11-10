# Dockerfile for Axum Backend
# Optimized multi-stage build for Fly.io

# Use Rust nightly slim (required for edition 2024)
FROM rustlang/rust:nightly-slim as builder

WORKDIR /app

# Install build dependencies (slim image needs more packages)
RUN apt-get update && \
    apt-get install -y \
    pkg-config \
    libssl-dev \
    build-essential \
    libopencv-dev \
    libopencv-core-dev \
    libopencv-imgproc-dev \
    libopencv-highgui-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy workspace and frontend
COPY Cargo.toml Cargo.lock ./
COPY axum-backend ./axum-backend/
COPY vision-classifier ./vision-classifier/
COPY unified-detector ./unified-detector/
COPY leptos-frontend ./leptos-frontend/

# Create dummy packages for other workspace members (needed before trunk build)
RUN for pkg in tauri-stretch hf-floorplan-loader validation-pipeline test-floorplan vtracer-test room-detection-rust; do \
        mkdir -p $pkg/src && \
        echo 'fn main() {}' > $pkg/src/main.rs && \
        printf '[package]\nname = "%s"\nversion = "0.1.0"\nedition = "2021"\n\n[dependencies]\nserde = { workspace = true }\nserde_json = { workspace = true }\n' "$pkg" > $pkg/Cargo.toml; \
    done

# Install wasm32 target for Leptos frontend
RUN rustup target add wasm32-unknown-unknown

# Build leptos-frontend with Trunk
RUN apt-get update && apt-get install -y wget && \
    wget -qO- https://github.com/trunk-rs/trunk/releases/download/v0.21.14/trunk-x86_64-unknown-linux-gnu.tar.gz | tar xz && \
    mv trunk /usr/local/bin/ && \
    cd leptos-frontend && trunk build --release && cd ..

# Build the actual application
RUN cargo build --release --bin axum-backend

# Copy frontend dist files to the backend's static file directory
RUN cp -r leptos-frontend/dist/* target/release/

# Runtime stage - compatible slim image
FROM rustlang/rust:nightly-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && \
    apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Copy the binary from builder
COPY --from=builder /app/target/release/axum-backend /app/axum-backend

# Copy the frontend static files (they're in the same directory as the binary now)
COPY --from=builder /app/target/release/*.html /app/
COPY --from=builder /app/target/release/*.js /app/
COPY --from=builder /app/target/release/*.wasm /app/

# Set environment variables
ENV RUST_LOG=info
ENV PORT=8080

# Expose the port
EXPOSE 8080

# Run the binary
CMD ["/app/axum-backend"]
