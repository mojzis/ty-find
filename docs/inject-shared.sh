#!/usr/bin/env bash
# Injects shared content into README.md and docs/src/setup.md.
#
# Shared source files live in docs/shared/. Each target file uses
# HTML-comment markers to delimit the region that gets replaced:
#
#   <!-- BEGIN SHARED:claude-snippet -->
#   ...replaced on each run...
#   <!-- END SHARED:claude-snippet -->
#
# Usage:
#   ./docs/inject-shared.sh          # inject into all targets
#   ./docs/inject-shared.sh --check  # exit 1 if any target is stale

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SHARED_DIR="$SCRIPT_DIR/shared"
CHECK_MODE=false

if [[ "${1:-}" == "--check" ]]; then
    CHECK_MODE=true
fi

FENCE='```'

# inject TARGET KEY SOURCE WRAPPER
#   WRAPPER: "codefence-markdown" wraps content in ```markdown ... ```
#            "none" injects content as-is
inject() {
    local target="$1"
    local key="$2"
    local source="$3"
    local wrapper="${4:-none}"

    local begin="<!-- BEGIN SHARED:$key -->"
    local end="<!-- END SHARED:$key -->"

    if ! grep -qF "$begin" "$target"; then
        echo "ERROR: marker '$begin' not found in $target" >&2
        exit 1
    fi

    local tmpfile
    tmpfile=$(mktemp)

    awk \
        -v begin="$begin" \
        -v end_marker="$end" \
        -v source="$source" \
        -v wrapper="$wrapper" \
        -v fence="$FENCE" \
    'index($0, begin) {
        print begin
        if (wrapper == "codefence-markdown") print fence "markdown"
        while ((getline line < source) > 0) print line
        close(source)
        if (wrapper == "codefence-markdown") print fence
        skip = 1
        next
    }
    index($0, end_marker) {
        skip = 0
        print end_marker
        next
    }
    !skip { print }' "$target" > "$tmpfile"

    if $CHECK_MODE; then
        if ! diff -q "$tmpfile" "$target" > /dev/null 2>&1; then
            echo "STALE: $target (run ./docs/inject-shared.sh to update)" >&2
            rm -f "$tmpfile"
            return 1
        fi
        echo "OK: $target is up to date"
    else
        cp "$tmpfile" "$target"
        echo "Injected $key into $target"
    fi
    rm -f "$tmpfile"
}

stale=0

inject "$REPO_ROOT/README.md" \
    "claude-snippet" \
    "$SHARED_DIR/claude-snippet.md" \
    "codefence-markdown" || stale=1

inject "$REPO_ROOT/docs/src/setup.md" \
    "claude-snippet" \
    "$SHARED_DIR/claude-snippet.md" \
    "codefence-markdown" || stale=1

if $CHECK_MODE && [[ $stale -ne 0 ]]; then
    exit 1
fi

echo "Done."
