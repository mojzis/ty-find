#!/usr/bin/env bash
set -euo pipefail

# Smoke test for LSP race condition reliability.
# Clones pandas + django, then runs tyf commands with cold-start daemon restarts
# to verify results are consistent across runs and workspaces.
#
# Usage:
#   benchmarks/smoke.sh              # run with cargo build (debug)
#   benchmarks/smoke.sh --release    # run with cargo build --release
#   benchmarks/smoke.sh /path/to/tyf # run with a specific binary

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

PANDAS_COMMIT="990a2ad7bdca09cd42a4998a60c8ece8677b4a15"
DJANGO_TAG="5.1.4"
PANDAS_DIR="${TMPDIR:-/tmp}/tyf-bench-pandas"
DJANGO_DIR="${TMPDIR:-/tmp}/tyf-bench-django"

COLD_RUNS=5      # number of cold-start iterations per test
PASS=0
FAIL=0
TOTAL=0

# --- Resolve tyf binary ---
if [ "${1:-}" = "--release" ]; then
    echo "Building release binary..."
    cargo build --release --manifest-path "$PROJECT_DIR/Cargo.toml" --quiet
    TYF="$PROJECT_DIR/target/release/tyf"
elif [ -n "${1:-}" ] && [ -f "${1:-}" ]; then
    TYF="$1"
else
    echo "Building debug binary..."
    cargo build --manifest-path "$PROJECT_DIR/Cargo.toml" --quiet
    TYF="$PROJECT_DIR/target/debug/tyf"
fi

echo "Using binary: $TYF"
echo ""

# --- Clone repos ---
clone_repo() {
    local dir="$1" name="$2" url="$3" ref="$4"
    if [ -d "$dir/.git" ]; then
        echo "$name already cloned at $dir"
    else
        echo "Cloning $name..."
        rm -rf "$dir"
        git init "$dir" --quiet
        git -C "$dir" fetch --depth 1 "$url" "$ref" --quiet
        git -C "$dir" checkout FETCH_HEAD --quiet
        echo "$name cloned at $dir"
    fi
}

clone_repo "$PANDAS_DIR" "pandas" "https://github.com/pandas-dev/pandas.git" "$PANDAS_COMMIT"
clone_repo "$DJANGO_DIR" "django" "https://github.com/django/django.git" "refs/tags/$DJANGO_TAG"
echo ""

# --- Test helpers ---
stop_daemon() {
    "$TYF" daemon stop >/dev/null 2>&1 || true
    sleep 0.3
}

assert_output() {
    local label="$1" expected="$2" actual="$3"
    TOTAL=$((TOTAL + 1))
    if echo "$actual" | grep -qF "$expected"; then
        PASS=$((PASS + 1))
    else
        FAIL=$((FAIL + 1))
        echo "  FAIL: $label"
        echo "    expected to contain: $expected"
        echo "    got: $(echo "$actual" | head -3)"
    fi
}

assert_line_count() {
    local label="$1" min="$2" pattern="$3" output="$4"
    TOTAL=$((TOTAL + 1))
    local count
    count=$(echo "$output" | grep -c "$pattern" || true)
    if [ "$count" -ge "$min" ]; then
        PASS=$((PASS + 1))
    else
        FAIL=$((FAIL + 1))
        echo "  FAIL: $label"
        echo "    expected >= $min lines matching '$pattern', got $count"
    fi
}

# --- Cold-start reliability (pandas) ---
echo "=== Cold-start reliability: pandas ==="
for i in $(seq 1 $COLD_RUNS); do
    stop_daemon
    output=$(cd "$PANDAS_DIR" && "$TYF" find DataFrame 2>&1)
    assert_output "pandas find DataFrame (run $i)" "Found 3 definition(s)" "$output"
done
echo "  pandas cold-start: done ($COLD_RUNS runs)"
echo ""

# --- Cold-start reliability (django) ---
echo "=== Cold-start reliability: django ==="
for i in $(seq 1 $COLD_RUNS); do
    stop_daemon
    output=$(cd "$DJANGO_DIR" && "$TYF" find QuerySet 2>&1)
    assert_output "django find QuerySet (run $i)" "Found 1 definition(s)" "$output"
done
echo "  django cold-start: done ($COLD_RUNS runs)"
echo ""

# --- Multi-workspace switching (warm daemon) ---
echo "=== Multi-workspace switching ==="
stop_daemon

output=$(cd "$PANDAS_DIR" && "$TYF" find DataFrame 2>&1)
assert_output "pandas DataFrame (warm 1)" "Found 3 definition(s)" "$output"

output=$(cd "$DJANGO_DIR" && "$TYF" find QuerySet 2>&1)
assert_output "django QuerySet (warm 1)" "Found 1 definition(s)" "$output"

output=$(cd "$PANDAS_DIR" && "$TYF" find Series 2>&1)
assert_output "pandas Series (warm 2)" "Found 2 definition(s)" "$output"

output=$(cd "$DJANGO_DIR" && "$TYF" find HttpRequest 2>&1)
assert_output "django HttpRequest (warm 2)" "Found 1 definition(s)" "$output"

output=$(cd "$PANDAS_DIR" && "$TYF" find Index 2>&1)
assert_output "pandas Index (warm 3)" "Found 1 definition(s)" "$output"

output=$(cd "$DJANGO_DIR" && "$TYF" find HttpResponse 2>&1)
assert_output "django HttpResponse (warm 3)" "Found 1 definition(s)" "$output"

echo "  workspace switching: done"
echo ""

# --- All commands (pandas) ---
echo "=== All commands: pandas ==="
stop_daemon

output=$(cd "$PANDAS_DIR" && "$TYF" list pandas/core/frame.py 2>&1)
assert_output "pandas list" "DataFrame (Class)" "$output"

output=$(cd "$PANDAS_DIR" && "$TYF" find DataFrame Series 2>&1)
assert_output "pandas find multi (DataFrame)" "Found 3 definition(s)" "$output"
assert_output "pandas find multi (Series)" "Found 2 definition(s)" "$output"

output=$(cd "$PANDAS_DIR" && "$TYF" find DataFrame --fuzzy 2>&1)
assert_line_count "pandas find fuzzy" 10 "DataFrame" "$output"

output=$(cd "$PANDAS_DIR" && "$TYF" inspect DataFrame 2>&1)
assert_output "pandas inspect hover" "Two-dimensional" "$output"

output=$(cd "$PANDAS_DIR" && "$TYF" refs read_csv 2>&1)
assert_output "pandas refs" "reference(s) for: 'read_csv'" "$output"

echo "  all commands: done"
echo ""

# --- All commands (django) ---
echo "=== All commands: django ==="
stop_daemon

output=$(cd "$DJANGO_DIR" && "$TYF" list django/db/models/query.py 2>&1)
assert_output "django list" "QuerySet" "$output"

output=$(cd "$DJANGO_DIR" && "$TYF" find QuerySet HttpResponse 2>&1)
assert_output "django find multi (QuerySet)" "Found 1 definition(s)" "$output"
assert_output "django find multi (HttpResponse)" "Found 1 definition(s)" "$output"

output=$(cd "$DJANGO_DIR" && "$TYF" inspect QuerySet 2>&1)
assert_output "django inspect" "Def" "$output"

output=$(cd "$DJANGO_DIR" && "$TYF" refs reverse 2>&1)
assert_output "django refs" "reference(s) for: 'reverse'" "$output"

echo "  all commands: done"
echo ""

# --- Cleanup ---
stop_daemon

# --- Summary ---
echo "==============================="
echo "Results: $PASS/$TOTAL passed, $FAIL failed"
echo "==============================="

if [ "$FAIL" -gt 0 ]; then
    exit 1
fi
