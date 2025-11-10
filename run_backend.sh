#!/bin/bash

# Set LLVM environment variables for OpenCV build
export LLVM_CONFIG_PATH=/opt/homebrew/opt/llvm/bin/llvm-config
export LIBCLANG_PATH=/opt/homebrew/opt/llvm/lib
export DYLD_LIBRARY_PATH=/opt/homebrew/opt/llvm/lib:$DYLD_LIBRARY_PATH

# Run the backend
cargo run --release --bin axum-backend
