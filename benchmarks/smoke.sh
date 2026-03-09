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
# Exercises all commands interleaved across 4 workspaces (pandas, django,
# test_project, test_project2) on a single warm daemon, verifying that
# switching workspaces doesn't break path resolution or LSP state.
echo "=== Multi-workspace switching ==="
stop_daemon

TP1="$PROJECT_DIR/test_project"
TP2="$PROJECT_DIR/test_project2"

# -- find (interleaved across all 4 workspaces) --

output=$(cd "$PANDAS_DIR" && "$TYF" find DataFrame 2>&1)
assert_output "pandas find DataFrame" "Found 3 definition(s)" "$output"

output=$(cd "$DJANGO_DIR" && "$TYF" find QuerySet 2>&1)
assert_output "django find QuerySet" "Found 1 definition(s)" "$output"

output=$("$TYF" --workspace "$TP1" find Animal 2>&1)
assert_output "test_project find Animal" "models.py" "$output"

output=$("$TYF" --workspace "$TP2" find UserService 2>&1)
assert_output "test_project2 find UserService" "services.py" "$output"

# switch back to large repos
output=$(cd "$PANDAS_DIR" && "$TYF" find Series 2>&1)
assert_output "pandas find Series (back)" "Found 2 definition(s)" "$output"

output=$(cd "$DJANGO_DIR" && "$TYF" find HttpRequest 2>&1)
assert_output "django find HttpRequest (back)" "Found 1 definition(s)" "$output"

output=$("$TYF" --workspace "$TP1" find create_dog 2>&1)
assert_output "test_project find create_dog (back)" "models.py" "$output"

# -- find --fuzzy --

output=$(cd "$PANDAS_DIR" && "$TYF" find DataFrame --fuzzy 2>&1)
assert_line_count "pandas find fuzzy DataFrame" 10 "DataFrame" "$output"

output=$("$TYF" --workspace "$TP1" find Dog --fuzzy 2>&1)
assert_output "test_project find fuzzy Dog" "Dog" "$output"

output=$(cd "$DJANGO_DIR" && "$TYF" find QuerySet --fuzzy 2>&1)
assert_output "django find fuzzy QuerySet" "QuerySet" "$output"

output=$("$TYF" --workspace "$TP2" find User --fuzzy 2>&1)
assert_output "test_project2 find fuzzy User" "User" "$output"

# -- inspect --

output=$(cd "$PANDAS_DIR" && "$TYF" inspect DataFrame 2>&1)
assert_output "pandas inspect DataFrame" "Def" "$output"

output=$("$TYF" --workspace "$TP2" inspect User 2>&1)
assert_output "test_project2 inspect User" "services.py" "$output"

output=$(cd "$DJANGO_DIR" && "$TYF" inspect QuerySet 2>&1)
assert_output "django inspect QuerySet" "Def" "$output"

output=$("$TYF" --workspace "$TP1" inspect Animal 2>&1)
assert_output "test_project inspect Animal" "models.py" "$output"

# -- refs --

output=$(cd "$PANDAS_DIR" && "$TYF" refs read_csv 2>&1)
assert_output "pandas refs read_csv" "reference(s) for: 'read_csv'" "$output"

output=$("$TYF" --workspace "$TP1" refs Animal 2>&1)
assert_output "test_project refs Animal" "reference(s) for: 'Animal'" "$output"

output=$(cd "$DJANGO_DIR" && "$TYF" refs reverse 2>&1)
assert_output "django refs reverse" "reference(s) for: 'reverse'" "$output"

output=$("$TYF" --workspace "$TP2" refs User 2>&1)
assert_output "test_project2 refs User" "reference(s) for: 'User'" "$output"

# -- members --

output=$("$TYF" --workspace "$TP1" members Animal --file "$TP1/models.py" 2>&1)
assert_output "test_project members Animal" "speak" "$output"

output=$("$TYF" --workspace "$TP2" members User --file "$TP2/services.py" 2>&1)
assert_output "test_project2 members User" "display_name" "$output"

# -- list --

output=$(cd "$PANDAS_DIR" && "$TYF" list pandas/core/frame.py 2>&1)
assert_output "pandas list frame.py" "DataFrame (Class)" "$output"

output=$("$TYF" --workspace "$TP1" list "$TP1/models.py" 2>&1)
assert_output "test_project list models.py" "Animal" "$output"

output=$(cd "$DJANGO_DIR" && "$TYF" list django/db/models/query.py 2>&1)
assert_output "django list query.py" "QuerySet" "$output"

output=$("$TYF" --workspace "$TP2" list "$TP2/services.py" 2>&1)
assert_output "test_project2 list services.py" "UserService" "$output"

# -- daemon status should show all 4 workspaces --
output=$("$TYF" daemon status 2>&1)
assert_output "daemon status shows test_project" "test_project" "$output"
assert_output "daemon status shows test_project2" "test_project2" "$output"
assert_output "daemon status shows PID" "PID:" "$output"
assert_output "daemon status shows Working dir" "Working dir:" "$output"

echo "  workspace switching: done"
echo ""

# --- Nonexistent symbol (rg early termination) ---
# Verifies that looking up a symbol that doesn't exist completes quickly
# (rg circuit-breaker skips the 3-second retry chain) across all commands
# and all 4 workspaces.
echo "=== Nonexistent symbol (rg early termination) ==="
BOGUS="this_symbol_absolutely_does_not_exist_xyz_98765"

assert_fast() {
    local label="$1" max_ms="$2" cmd="$3"
    TOTAL=$((TOTAL + 1))
    local start end elapsed_ms
    start=$(date +%s%N 2>/dev/null || python3 -c 'import time; print(int(time.time()*1e9))')
    eval "$cmd" >/dev/null 2>&1 || true
    end=$(date +%s%N 2>/dev/null || python3 -c 'import time; print(int(time.time()*1e9))')
    elapsed_ms=$(( (end - start) / 1000000 ))
    if [ "$elapsed_ms" -le "$max_ms" ]; then
        PASS=$((PASS + 1))
    else
        FAIL=$((FAIL + 1))
        echo "  FAIL: $label"
        echo "    expected <= ${max_ms}ms, took ${elapsed_ms}ms"
    fi
}

# -- find nonexistent across all 4 workspaces --

output=$(cd "$PANDAS_DIR" && "$TYF" find "$BOGUS" 2>&1) || true
assert_output "pandas find nonexistent" "No results found" "$output"

output=$(cd "$DJANGO_DIR" && "$TYF" find "$BOGUS" 2>&1) || true
assert_output "django find nonexistent" "No results found" "$output"

output=$("$TYF" --workspace "$TP1" find "$BOGUS" 2>&1) || true
assert_output "test_project find nonexistent" "No results found" "$output"

output=$("$TYF" --workspace "$TP2" find "$BOGUS" 2>&1) || true
assert_output "test_project2 find nonexistent" "No results found" "$output"

# -- inspect nonexistent across all 4 workspaces --

output=$(cd "$PANDAS_DIR" && "$TYF" inspect "$BOGUS" 2>&1) || true
assert_output "pandas inspect nonexistent" "No results found" "$output"

output=$(cd "$DJANGO_DIR" && "$TYF" inspect "$BOGUS" 2>&1) || true
assert_output "django inspect nonexistent" "No results found" "$output"

output=$("$TYF" --workspace "$TP1" inspect "$BOGUS" 2>&1) || true
assert_output "test_project inspect nonexistent" "No results found" "$output"

output=$("$TYF" --workspace "$TP2" inspect "$BOGUS" 2>&1) || true
assert_output "test_project2 inspect nonexistent" "No results found" "$output"

# -- refs nonexistent across all 4 workspaces --

output=$(cd "$PANDAS_DIR" && "$TYF" refs "$BOGUS" 2>&1) || true
assert_output "pandas refs nonexistent" "No results found" "$output"

output=$(cd "$DJANGO_DIR" && "$TYF" refs "$BOGUS" 2>&1) || true
assert_output "django refs nonexistent" "No results found" "$output"

output=$("$TYF" --workspace "$TP1" refs "$BOGUS" 2>&1) || true
assert_output "test_project refs nonexistent" "No results found" "$output"

output=$("$TYF" --workspace "$TP2" refs "$BOGUS" 2>&1) || true
assert_output "test_project2 refs nonexistent" "No results found" "$output"

# -- timing: large repos should be fast with rg early termination --
# Generous 3000ms threshold (without rg it would take ~6+ seconds per call
# due to the full retry chain)
assert_fast "pandas find nonexistent (timing)" 3000 \
    "cd '$PANDAS_DIR' && '$TYF' find '$BOGUS'"
assert_fast "django find nonexistent (timing)" 3000 \
    "cd '$DJANGO_DIR' && '$TYF' find '$BOGUS'"

echo "  nonexistent symbol: done"
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
assert_output "pandas inspect" "Def" "$output"

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
