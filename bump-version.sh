#!/bin/bash
set -e

if [ -z "$1" ]; then
    echo "Usage: ./bump-version.sh [patch|minor|major|x.y.z]"
    exit 1
fi

# Get current version from Cargo.toml
CURRENT_VERSION=$(grep '^version = ' Cargo.toml | head -1 | cut -d'"' -f2)
echo "Current version: $CURRENT_VERSION"

# Determine new version
if [[ "$1" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    NEW_VERSION="$1"
else
    # Install cargo-bump if not present
    if ! command -v cargo-bump &> /dev/null; then
        echo "Installing cargo-bump..."
        cargo install cargo-bump
    fi
    
    # Use cargo-bump to calculate new version
    cargo bump "$1" --dry-run > /tmp/bump-output 2>&1
    NEW_VERSION=$(grep 'Bumped version' /tmp/bump-output | cut -d' ' -f6)
fi

echo "New version: $NEW_VERSION"

# Update all version references
sed -i.bak "s/version = \"$CURRENT_VERSION\"/version = \"$NEW_VERSION\"/" Cargo.toml
sed -i.bak "s/version=\"$CURRENT_VERSION\"/version=\"$NEW_VERSION\"/" setup.py
sed -i.bak "s/__version__ = \"$CURRENT_VERSION\"/__version__ = \"$NEW_VERSION\"/" python/ty_find/__init__.py

# Clean up backup files
rm -f Cargo.toml.bak setup.py.bak python/ty_find/__init__.py.bak

echo "Version bumped to $NEW_VERSION in all files"

# Optional: commit and tag
read -p "Commit and tag? (y/n) " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    git add Cargo.toml setup.py python/ty_find/__init__.py
    git commit -m "bump version $CURRENT_VERSION -> $NEW_VERSION"
    git tag "v$NEW_VERSION"
    echo "Committed and tagged as v$NEW_VERSION"
fi