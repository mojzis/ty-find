#!/usr/bin/env bash
# Syncs the version badge shown in the docs menu bar with the crate version.
#
# Reads the version from Cargo.toml (the single source of truth) and rewrites
# the VERSION constant in docs/version.js. Run automatically by the docs deploy
# workflow before `mdbook build`, so the published site always matches Cargo.toml.
#
# Usage:
#   ./docs/gen-version.sh          # update docs/version.js
#   ./docs/gen-version.sh --check  # exit 1 if docs/version.js is stale
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
TARGET="$SCRIPT_DIR/version.js"

VERSION=$(grep -m1 '^version' "$REPO_ROOT/Cargo.toml" | sed -E 's/^version *= *"([^"]+)".*/\1/')
if [[ -z "$VERSION" ]]; then
    echo "ERROR: could not read version from $REPO_ROOT/Cargo.toml" >&2
    exit 1
fi

if [[ "${1:-}" == "--check" ]]; then
    if grep -qF "var VERSION = \"$VERSION\";" "$TARGET"; then
        echo "OK: docs version badge is v$VERSION"
        exit 0
    fi
    echo "STALE: $TARGET (run ./docs/gen-version.sh to update to v$VERSION)" >&2
    exit 1
fi

sed -i.bak -E "s/(var VERSION = \")[^\"]*(\";)/\1${VERSION}\2/" "$TARGET"
rm -f "$TARGET.bak"
echo "Set docs version badge to v$VERSION"
