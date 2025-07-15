#!/bin/bash

set -e

echo "Installing ty-find..."

# Check if Rust is installed
if ! command -v rustc &> /dev/null; then
    echo "Error: Rust is not installed. Please install Rust first:"
    echo "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    exit 1
fi

# Check if ty is installed
if ! command -v ty &> /dev/null; then
    echo "Error: ty is not installed. Please install ty first:"
    echo "pip install ty"
    exit 1
fi

# Build the project
cargo build --release

echo "ty-find built successfully!"
echo "Run './target/release/ty-find --help' to see usage instructions"