#!/usr/bin/env bash
# Generates llms.txt and llms-full.txt from mdBook source files,
# and copies source .md files into the build output for direct access.
set -euo pipefail

DOCS_DIR="$(cd "$(dirname "$0")" && pwd)"
SRC_DIR="$DOCS_DIR/src"
BOOK_DIR="$DOCS_DIR/book/html"
SITE_URL="https://mojzis.github.io/ty-find"

mkdir -p "$BOOK_DIR"

# Copy source .md files into the build output so they're served alongside HTML.
# Preserve directory structure (e.g. commands/*.md).
grep -oP '\(([^)]+\.md)\)' "$SRC_DIR/SUMMARY.md" | tr -d '()' | while IFS= read -r path; do
    if [ -f "$SRC_DIR/$path" ]; then
        mkdir -p "$BOOK_DIR/$(dirname "$path")"
        cp "$SRC_DIR/$path" "$BOOK_DIR/$path"
    fi
done
echo "Copied .md source files to $BOOK_DIR/"

# llms.txt — index with links to .md files
cat > "$BOOK_DIR/llms.txt" << EOF
# ty-find

> Type-aware Python code navigation for AI coding agents, powered by ty's LSP server.

## Documentation

EOF

# Parse SUMMARY.md to extract chapter links and titles.
# Matches both standalone links like "[Title](path.md)" and list items like "- [Title](path.md)".
grep -oP '\[([^\]]+)\]\(([^)]+)\)' "$SRC_DIR/SUMMARY.md" | while IFS= read -r match; do
    title=$(echo "$match" | sed 's/\[\([^]]*\)\](.*)/\1/')
    path=$(echo "$match" | sed 's/\[.*\](\(.*\))/\1/')
    echo "- [$title]($SITE_URL/$path): $title" >> "$BOOK_DIR/llms.txt"
done

echo "Wrote $BOOK_DIR/llms.txt"

# llms-full.txt — all source files concatenated
: > "$BOOK_DIR/llms-full.txt"
echo "# ty-find — Full Documentation" >> "$BOOK_DIR/llms-full.txt"
echo "" >> "$BOOK_DIR/llms-full.txt"

# Concatenate all .md files from SUMMARY.md in order
grep -oP '\(([^)]+\.md)\)' "$SRC_DIR/SUMMARY.md" | tr -d '()' | while IFS= read -r path; do
    if [ -f "$SRC_DIR/$path" ]; then
        cat "$SRC_DIR/$path" >> "$BOOK_DIR/llms-full.txt"
        printf '\n---\n\n' >> "$BOOK_DIR/llms-full.txt"
    fi
done

echo "Wrote $BOOK_DIR/llms-full.txt"
