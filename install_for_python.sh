#!/bin/bash
# Install script for ty-find that works with Python environments

set -e

echo "Installing ty-find..."

# Check if we're in a Python virtual environment
if [[ -n "$VIRTUAL_ENV" ]]; then
    INSTALL_DIR="$VIRTUAL_ENV/bin"
    echo "Installing to virtual environment: $VIRTUAL_ENV"
elif [[ -n "$CONDA_PREFIX" ]]; then
    INSTALL_DIR="$CONDA_PREFIX/bin"
    echo "Installing to conda environment: $CONDA_PREFIX"
else
    INSTALL_DIR="$HOME/.local/bin"
    echo "Installing to user local: $INSTALL_DIR"
    mkdir -p "$INSTALL_DIR"
fi

# Build the project
echo "Building ty-find..."
cargo build --release

# Copy binary to the appropriate location
cp target/release/ty-find "$INSTALL_DIR/"

echo "ty-find installed successfully to $INSTALL_DIR"
echo "Make sure $INSTALL_DIR is in your PATH"

# Test the installation
if command -v ty-find &> /dev/null; then
    echo "✓ ty-find is available in PATH"
    ty-find --help
else
    echo "⚠ ty-find is not in PATH. You may need to add $INSTALL_DIR to your PATH"
fi