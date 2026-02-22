#!/usr/bin/env bash
set -euo pipefail

# ty-find performance benchmark script
# Runs ty-find against a pandas checkout and measures performance.
# Usage:
#   benchmarks/run.sh                  # compare against baseline
#   benchmarks/run.sh --save-baseline  # save results as new baseline

PANDAS_COMMIT="990a2ad7bdca09cd42a4998a60c8ece8677b4a15"
HYPERFINE_RUNS=3
THRESHOLD="1.5"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BASELINE_FILE="$SCRIPT_DIR/baseline.json"

SAVE_BASELINE=false
if [ "${1:-}" = "--save-baseline" ]; then
    SAVE_BASELINE=true
fi

# --- Dependency checks ---
for cmd in ty-find hyperfine jq bc; do
    if ! command -v "$cmd" >/dev/null 2>&1; then
        echo "ERROR: '$cmd' is required but not found on PATH." >&2
        exit 1
    fi
done

# --- Clone pandas at pinned commit ---
PANDAS_DIR="${TMPDIR:-/tmp}/ty-find-bench-pandas"
if [ -d "$PANDAS_DIR/.git" ]; then
    echo "Pandas checkout already exists at $PANDAS_DIR, skipping clone."
    CURRENT_COMMIT="$(git -C "$PANDAS_DIR" rev-parse HEAD)"
    if [ "$CURRENT_COMMIT" != "$PANDAS_COMMIT" ]; then
        echo "WARNING: Existing checkout is at $CURRENT_COMMIT, expected $PANDAS_COMMIT"
        echo "Removing and re-cloning..."
        rm -rf "$PANDAS_DIR"
        git clone --depth 1 https://github.com/pandas-dev/pandas.git "$PANDAS_DIR"
        git -C "$PANDAS_DIR" fetch --depth 1 origin "$PANDAS_COMMIT"
        git -C "$PANDAS_DIR" checkout "$PANDAS_COMMIT"
    fi
else
    echo "Cloning pandas at commit $PANDAS_COMMIT..."
    git clone --depth 1 https://github.com/pandas-dev/pandas.git "$PANDAS_DIR"
    CURRENT_COMMIT="$(git -C "$PANDAS_DIR" rev-parse HEAD)"
    if [ "$CURRENT_COMMIT" != "$PANDAS_COMMIT" ]; then
        git -C "$PANDAS_DIR" fetch --depth 1 origin "$PANDAS_COMMIT"
        git -C "$PANDAS_DIR" checkout "$PANDAS_COMMIT"
    fi
fi

echo "Using pandas checkout at: $PANDAS_DIR"
echo ""

# --- Helper functions ---
TMPDIR_BENCH="$(mktemp -d)"
cleanup() {
    ty-find daemon stop >/dev/null 2>&1 || true
    rm -rf "$TMPDIR_BENCH"
}
trap cleanup EXIT

extract_median() {
    jq '.results[0].median' "$1"
}

run_hyperfine_bench() {
    local name="$1"
    local cmd="$2"
    local outfile="$TMPDIR_BENCH/${name}.json"
    echo "  Running: $cmd" >&2
    if hyperfine --warmup 1 --runs "$HYPERFINE_RUNS" --ignore-failure --export-json "$outfile" "$cmd" 2>&1 | \
        sed 's/^/    /' >&2; then
        if [ -f "$outfile" ] && jq -e '.results[0].median' "$outfile" >/dev/null 2>&1; then
            extract_median "$outfile"
            return 0
        fi
    fi
    echo "null"
    return 1
}

measure_single_run() {
    local cmd="$1"
    local start end elapsed result
    start="$(date +%s%N 2>/dev/null || python3 -c 'import time; print(int(time.time()*1e9))')"
    eval "$cmd" >/dev/null 2>&1 || true
    end="$(date +%s%N 2>/dev/null || python3 -c 'import time; print(int(time.time()*1e9))')"
    elapsed=$(( end - start ))
    result="$(echo "scale=6; $elapsed / 1000000000" | bc)"
    # bc omits leading zero for values < 1, add it for valid JSON
    case "$result" in
        .*) echo "0$result" ;;
        *)  echo "$result" ;;
    esac
}

get_platform() {
    local os arch
    os="$(uname -s)"
    arch="$(uname -m)"
    echo "$os $arch"
}

# --- Stop daemon for cold-start measurement ---
echo "=== Stopping daemon for clean benchmarks ==="
ty-find daemon stop >/dev/null 2>&1 || true
sleep 1

# --- Run benchmarks ---
echo ""
echo "=== Running ty-find benchmarks against pandas ==="
echo ""

# Store results in temp files since associative arrays and subshells don't mix well
RESULTS_DIR="$TMPDIR_BENCH/results"
mkdir -p "$RESULTS_DIR"

# Benchmark definitions (pipe-separated: name|ty_cmd|grep_cmd)
BENCHMARKS=(
    "find-DataFrame|ty-find --workspace $PANDAS_DIR find DataFrame|grep -rn 'class DataFrame' --include='*.py' $PANDAS_DIR"
    "find-Series|ty-find --workspace $PANDAS_DIR find Series|grep -rn 'class Series' --include='*.py' $PANDAS_DIR"
    "find-multi|ty-find --workspace $PANDAS_DIR find DataFrame Series Index|"
    "inspect-DataFrame|ty-find --workspace $PANDAS_DIR inspect DataFrame|"
    "inspect-multi|ty-find --workspace $PANDAS_DIR inspect DataFrame Series|"
    "workspace-symbols|ty-find --workspace $PANDAS_DIR workspace-symbols --query DataFrame|grep -rn 'DataFrame' --include='*.py' $PANDAS_DIR"
)

for bench in "${BENCHMARKS[@]}"; do
    IFS='|' read -r name ty_cmd grep_cmd <<< "$bench"

    echo "--- Benchmark: $name ---"

    # ty-find measurement
    ty_median="$(run_hyperfine_bench "${name}-ty" "$ty_cmd")" || true
    echo "$ty_median" > "$RESULTS_DIR/${name}.ty_median"
    echo "$ty_cmd" > "$RESULTS_DIR/${name}.ty_cmd"
    if [ "$ty_median" != "null" ]; then
        echo "  ty-find median: ${ty_median}s"
    else
        echo "  ty-find: FAILED (command errored or timed out)"
    fi

    # grep measurement (if applicable)
    if [ -n "$grep_cmd" ]; then
        grep_median="$(run_hyperfine_bench "${name}-grep" "$grep_cmd")" || true
        echo "$grep_median" > "$RESULTS_DIR/${name}.grep_median"
        echo "  grep median: ${grep_median}s"
    else
        echo "null" > "$RESULTS_DIR/${name}.grep_median"
        echo "  grep: (no equivalent)"
    fi
    echo ""
done

# --- Cold/warm start measurement ---
echo "=== Measuring startup times ==="
echo ""

echo "Stopping daemon for cold start measurement..."
ty-find daemon stop >/dev/null 2>&1 || true
sleep 1

echo "Measuring cold start..."
COLD_START="$(measure_single_run "ty-find --workspace $PANDAS_DIR inspect DataFrame")"
echo "  Cold start: ${COLD_START}s"

echo "Measuring warm start..."
WARM_START="$(measure_single_run "ty-find --workspace $PANDAS_DIR inspect DataFrame")"
echo "  Warm start: ${WARM_START}s"
echo ""

# --- Build results JSON ---
TIMESTAMP="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
PLATFORM="$(get_platform)"

# Build benchmarks JSON object using jq for correctness
BENCHMARKS_JSON="{}"
for bench in "${BENCHMARKS[@]}"; do
    IFS='|' read -r name ty_cmd _ <<< "$bench"

    ty_median="$(cat "$RESULTS_DIR/${name}.ty_median")"
    grep_median="$(cat "$RESULTS_DIR/${name}.grep_median")"
    # Clean workspace path from command for portability
    clean_cmd="$(echo "$ty_cmd" | sed "s| --workspace $PANDAS_DIR||" | sed 's/  */ /g')"

    BENCHMARKS_JSON="$(echo "$BENCHMARKS_JSON" | jq \
        --arg name "$name" \
        --argjson ty_med "$ty_median" \
        --argjson grep_med "$grep_median" \
        --arg cmd "$clean_cmd" \
        '.[$name] = {ty_find_median_s: $ty_med, grep_median_s: $grep_med, command: $cmd}'
    )"
done

RESULTS_JSON="$(jq -n \
    --arg commit "$PANDAS_COMMIT" \
    --arg ts "$TIMESTAMP" \
    --argjson runs "$HYPERFINE_RUNS" \
    --arg platform "$PLATFORM" \
    --argjson benchmarks "$BENCHMARKS_JSON" \
    --argjson cold "$COLD_START" \
    --argjson warm "$WARM_START" \
    '{
        metadata: {
            pandas_commit: $commit,
            timestamp: $ts,
            hyperfine_runs: $runs,
            platform: $platform
        },
        benchmarks: $benchmarks,
        startup: {
            cold_start_s: $cold,
            warm_start_s: $warm
        }
    }'
)"

# --- Save or compare ---
if [ "$SAVE_BASELINE" = true ]; then
    echo "=== Saving baseline ==="
    echo "$RESULTS_JSON" > "$BASELINE_FILE"
    echo "Baseline saved to $BASELINE_FILE"
    echo ""
    echo "$RESULTS_JSON" | jq .
else
    echo "=== Current results ==="
    echo "$RESULTS_JSON" | jq .
    echo ""

    if [ ! -f "$BASELINE_FILE" ]; then
        echo "No baseline.json found. Run with --save-baseline to create one."
        exit 0
    fi

    echo "=== Comparison against baseline ==="
    echo ""

    FAILURES=0
    printf "%-25s %12s %12s %8s %8s\n" "Benchmark" "Current (s)" "Baseline (s)" "Ratio" "Status"
    printf "%-25s %12s %12s %8s %8s\n" "-------------------------" "------------" "------------" "--------" "--------"

    for bench in "${BENCHMARKS[@]}"; do
        IFS='|' read -r name _ _ <<< "$bench"

        current="$(cat "$RESULTS_DIR/${name}.ty_median")"
        baseline="$(jq -r ".benchmarks[\"$name\"].ty_find_median_s" "$BASELINE_FILE")"

        # Skip if either current or baseline is null
        if [ "$current" = "null" ] || [ "$baseline" = "null" ] || [ -z "$baseline" ]; then
            printf "%-25s %12s %12s %8s %8s\n" "$name" "${current}" "${baseline}" "N/A" "SKIP"
            continue
        fi

        ratio="$(echo "scale=4; $current / $baseline" | bc)"
        exceeds="$(echo "$ratio > $THRESHOLD" | bc)"

        if [ "$exceeds" -eq 1 ]; then
            status="FAIL"
            FAILURES=$((FAILURES + 1))
        else
            status="PASS"
        fi

        printf "%-25s %12.4f %12.4f %8.2fx %8s\n" "$name" "$current" "$baseline" "$ratio" "$status"
    done

    # Also compare startup times
    echo ""
    echo "Startup times (informational, not gated):"
    COLD_BASELINE="$(jq -r '.startup.cold_start_s' "$BASELINE_FILE")"
    WARM_BASELINE="$(jq -r '.startup.warm_start_s' "$BASELINE_FILE")"
    if [ "$COLD_BASELINE" != "null" ] && [ -n "$COLD_BASELINE" ]; then
        cold_ratio="$(echo "scale=4; $COLD_START / $COLD_BASELINE" | bc)"
        printf "  Cold start: %.4fs (baseline: %.4fs, ratio: %.2fx)\n" "$COLD_START" "$COLD_BASELINE" "$cold_ratio"
    fi
    if [ "$WARM_BASELINE" != "null" ] && [ -n "$WARM_BASELINE" ]; then
        warm_ratio="$(echo "scale=4; $WARM_START / $WARM_BASELINE" | bc)"
        printf "  Warm start: %.4fs (baseline: %.4fs, ratio: %.2fx)\n" "$WARM_START" "$WARM_BASELINE" "$warm_ratio"
    fi

    echo ""
    if [ "$FAILURES" -gt 0 ]; then
        echo "RESULT: FAIL ($FAILURES benchmark(s) exceeded ${THRESHOLD}x threshold)"
        exit 1
    else
        echo "RESULT: PASS (all benchmarks within ${THRESHOLD}x threshold)"
        exit 0
    fi
fi
