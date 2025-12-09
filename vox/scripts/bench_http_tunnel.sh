#!/bin/bash
#
# HTTP Tunnel Benchmark Script
#
# Measures the overhead of rapace TCP tunneling vs direct HTTP.
# Uses oha for load generation.
#
# Usage:
#   ./scripts/bench_http_tunnel.sh
#
# Prerequisites:
#   - oha installed (https://github.com/hatoo/oha)
#   - Rust toolchain

set -e

# Configuration
HOST_PORT=4000
INTERNAL_PORT=9876
DURATION="10s"
CONCURRENCY_LEVELS="1 8 64 256"
RESULTS_DIR="bench_results"
UNIX_SOCKET="/tmp/rapace-tunnel.sock"
SHM_FILE="/tmp/rapace-tunnel.shm"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Get git info
GIT_COMMIT=$(git rev-parse --short HEAD 2>/dev/null || echo "unknown")
GIT_BRANCH=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "unknown")

echo "============================================"
echo "  HTTP Tunnel Benchmark"
echo "============================================"
echo ""
echo "Git commit: $GIT_COMMIT"
echo "Git branch: $GIT_BRANCH"
echo "Machine: $(uname -m)"
echo "OS: $(uname -s)"
echo "Date: $(date -Iseconds)"
echo ""

# Check for oha
if ! command -v oha &> /dev/null; then
    echo -e "${RED}Error: oha not found. Install with: cargo install oha${NC}"
    exit 1
fi

# Build release binaries
echo -e "${YELLOW}Building release binaries...${NC}"
cargo build --release -p rapace-http-tunnel 2>&1 | tail -1

# Create results directory
mkdir -p "$RESULTS_DIR"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
RUN_DIR="$RESULTS_DIR/$TIMESTAMP"
mkdir -p "$RUN_DIR"

# Save metadata
cat > "$RUN_DIR/metadata.json" << EOF
{
  "git_commit": "$GIT_COMMIT",
  "git_branch": "$GIT_BRANCH",
  "machine": "$(uname -m)",
  "os": "$(uname -s)",
  "date": "$(date -Iseconds)",
  "duration": "$DURATION",
  "concurrency_levels": "$CONCURRENCY_LEVELS"
}
EOF

# Helper function to wait for HTTP server
wait_for_server() {
    local port=$1
    local max_attempts=50
    local attempt=0
    while ! curl -s "http://127.0.0.1:$port/health" > /dev/null 2>&1; do
        attempt=$((attempt + 1))
        if [ $attempt -ge $max_attempts ]; then
            echo -e "${RED}Server on port $port didn't start in time${NC}"
            return 1
        fi
        sleep 0.1
    done
    return 0
}

# Helper function to run benchmark
run_bench() {
    local name=$1
    local url=$2
    local concurrency=$3
    local output_file="$RUN_DIR/${name}_c${concurrency}.json"

    echo "  Running: $name (c=$concurrency) -> $url"
    oha "$url" -z "$DURATION" -c "$concurrency" --json > "$output_file" 2>/dev/null

    # Extract key metrics
    local rps=$(jq -r '.summary.requestsPerSec' "$output_file")
    local p50=$(jq -r '.latencyPercentiles.p50' "$output_file")
    local p99=$(jq -r '.latencyPercentiles.p99' "$output_file")
    printf "    RPS: %.0f, p50: %.3fms, p99: %.3fms\n" "$rps" "$p50" "$p99"
}

# Cleanup function
cleanup() {
    echo ""
    echo -e "${YELLOW}Cleaning up...${NC}"
    # Kill any leftover processes
    pkill -f "http_baseline" 2>/dev/null || true
    pkill -f "http-tunnel-host" 2>/dev/null || true
    pkill -f "http-tunnel-plugin" 2>/dev/null || true
    rm -f "$UNIX_SOCKET" "$SHM_FILE" 2>/dev/null || true
    sleep 0.5
}

# Set up trap for cleanup
trap cleanup EXIT

# ============================================
# BASELINE BENCHMARK
# ============================================
echo ""
echo -e "${GREEN}=== Baseline (direct HTTP) ===${NC}"

cleanup

./target/release/http_baseline &
BASELINE_PID=$!
sleep 0.5

if ! wait_for_server $HOST_PORT; then
    echo -e "${RED}Baseline server failed to start${NC}"
    exit 1
fi

for size in small large; do
    echo ""
    echo "  Endpoint: /$size"
    for c in $CONCURRENCY_LEVELS; do
        run_bench "baseline_${size}" "http://127.0.0.1:$HOST_PORT/$size" "$c"
    done
done

kill $BASELINE_PID 2>/dev/null || true
wait $BASELINE_PID 2>/dev/null || true

# ============================================
# TUNNEL-STREAM BENCHMARK (Unix Socket)
# ============================================
echo ""
echo -e "${GREEN}=== Tunnel over Stream (Unix Socket) ===${NC}"

cleanup

# Start host first (it listens)
./target/release/http-tunnel-host --transport=stream --addr="$UNIX_SOCKET" &
HOST_PID=$!
sleep 0.5

# Start plugin (it connects)
./target/release/http-tunnel-plugin --transport=stream --addr="$UNIX_SOCKET" &
PLUGIN_PID=$!
sleep 1

if ! wait_for_server $HOST_PORT; then
    echo -e "${RED}Tunnel-stream server failed to start${NC}"
    kill $HOST_PID $PLUGIN_PID 2>/dev/null || true
    exit 1
fi

for size in small large; do
    echo ""
    echo "  Endpoint: /$size"
    for c in $CONCURRENCY_LEVELS; do
        run_bench "tunnel_stream_${size}" "http://127.0.0.1:$HOST_PORT/$size" "$c"
    done
done

kill $HOST_PID $PLUGIN_PID 2>/dev/null || true
wait $HOST_PID $PLUGIN_PID 2>/dev/null || true

# ============================================
# TUNNEL-SHM BENCHMARK
# ============================================
echo ""
echo -e "${GREEN}=== Tunnel over SHM ===${NC}"

cleanup

# Start host first (it creates SHM file)
./target/release/http-tunnel-host --transport=shm --addr="$SHM_FILE" &
HOST_PID=$!
sleep 1

# Start plugin (it opens SHM file)
./target/release/http-tunnel-plugin --transport=shm --addr="$SHM_FILE" &
PLUGIN_PID=$!
sleep 1

if ! wait_for_server $HOST_PORT; then
    echo -e "${RED}Tunnel-shm server failed to start${NC}"
    kill $HOST_PID $PLUGIN_PID 2>/dev/null || true
    exit 1
fi

for size in small large; do
    echo ""
    echo "  Endpoint: /$size"
    for c in $CONCURRENCY_LEVELS; do
        run_bench "tunnel_shm_${size}" "http://127.0.0.1:$HOST_PORT/$size" "$c"
    done
done

kill $HOST_PID $PLUGIN_PID 2>/dev/null || true
wait $HOST_PID $PLUGIN_PID 2>/dev/null || true

# ============================================
# GENERATE SUMMARY
# ============================================
echo ""
echo -e "${GREEN}=== Summary ===${NC}"
echo ""

# Generate summary table
echo "Results saved to: $RUN_DIR"
echo ""

# Create a summary markdown file
SUMMARY_FILE="$RUN_DIR/SUMMARY.md"

cat > "$SUMMARY_FILE" << EOF
# HTTP Tunnel Benchmark Results

**Date**: $(date -Iseconds)
**Commit**: $GIT_COMMIT
**Branch**: $GIT_BRANCH
**Duration**: $DURATION per test

## Small Response (2 bytes)

| Transport | Concurrency | RPS | p50 (ms) | p99 (ms) |
|-----------|-------------|-----|----------|----------|
EOF

for c in $CONCURRENCY_LEVELS; do
    if [ -f "$RUN_DIR/baseline_small_c${c}.json" ]; then
        rps=$(jq -r '.summary.requestsPerSec' "$RUN_DIR/baseline_small_c${c}.json")
        p50=$(jq -r '.latencyPercentiles.p50' "$RUN_DIR/baseline_small_c${c}.json")
        p99=$(jq -r '.latencyPercentiles.p99' "$RUN_DIR/baseline_small_c${c}.json")
        printf "| Baseline | %d | %.0f | %.3f | %.3f |\n" "$c" "$rps" "$p50" "$p99" >> "$SUMMARY_FILE"
    fi
done

for c in $CONCURRENCY_LEVELS; do
    if [ -f "$RUN_DIR/tunnel_stream_small_c${c}.json" ]; then
        rps=$(jq -r '.summary.requestsPerSec' "$RUN_DIR/tunnel_stream_small_c${c}.json")
        p50=$(jq -r '.latencyPercentiles.p50' "$RUN_DIR/tunnel_stream_small_c${c}.json")
        p99=$(jq -r '.latencyPercentiles.p99' "$RUN_DIR/tunnel_stream_small_c${c}.json")
        printf "| Stream | %d | %.0f | %.3f | %.3f |\n" "$c" "$rps" "$p50" "$p99" >> "$SUMMARY_FILE"
    fi
done

for c in $CONCURRENCY_LEVELS; do
    if [ -f "$RUN_DIR/tunnel_shm_small_c${c}.json" ]; then
        rps=$(jq -r '.summary.requestsPerSec' "$RUN_DIR/tunnel_shm_small_c${c}.json")
        p50=$(jq -r '.latencyPercentiles.p50' "$RUN_DIR/tunnel_shm_small_c${c}.json")
        p99=$(jq -r '.latencyPercentiles.p99' "$RUN_DIR/tunnel_shm_small_c${c}.json")
        printf "| SHM | %d | %.0f | %.3f | %.3f |\n" "$c" "$rps" "$p50" "$p99" >> "$SUMMARY_FILE"
    fi
done

cat >> "$SUMMARY_FILE" << EOF

## Large Response (~256KB)

| Transport | Concurrency | RPS | p50 (ms) | p99 (ms) |
|-----------|-------------|-----|----------|----------|
EOF

for c in $CONCURRENCY_LEVELS; do
    if [ -f "$RUN_DIR/baseline_large_c${c}.json" ]; then
        rps=$(jq -r '.summary.requestsPerSec' "$RUN_DIR/baseline_large_c${c}.json")
        p50=$(jq -r '.latencyPercentiles.p50' "$RUN_DIR/baseline_large_c${c}.json")
        p99=$(jq -r '.latencyPercentiles.p99' "$RUN_DIR/baseline_large_c${c}.json")
        printf "| Baseline | %d | %.0f | %.3f | %.3f |\n" "$c" "$rps" "$p50" "$p99" >> "$SUMMARY_FILE"
    fi
done

for c in $CONCURRENCY_LEVELS; do
    if [ -f "$RUN_DIR/tunnel_stream_large_c${c}.json" ]; then
        rps=$(jq -r '.summary.requestsPerSec' "$RUN_DIR/tunnel_stream_large_c${c}.json")
        p50=$(jq -r '.latencyPercentiles.p50' "$RUN_DIR/tunnel_stream_large_c${c}.json")
        p99=$(jq -r '.latencyPercentiles.p99' "$RUN_DIR/tunnel_stream_large_c${c}.json")
        printf "| Stream | %d | %.0f | %.3f | %.3f |\n" "$c" "$rps" "$p50" "$p99" >> "$SUMMARY_FILE"
    fi
done

for c in $CONCURRENCY_LEVELS; do
    if [ -f "$RUN_DIR/tunnel_shm_large_c${c}.json" ]; then
        rps=$(jq -r '.summary.requestsPerSec' "$RUN_DIR/tunnel_shm_large_c${c}.json")
        p50=$(jq -r '.latencyPercentiles.p50' "$RUN_DIR/tunnel_shm_large_c${c}.json")
        p99=$(jq -r '.latencyPercentiles.p99' "$RUN_DIR/tunnel_shm_large_c${c}.json")
        printf "| SHM | %d | %.0f | %.3f | %.3f |\n" "$c" "$rps" "$p50" "$p99" >> "$SUMMARY_FILE"
    fi
done

echo ""
echo "Summary written to: $SUMMARY_FILE"
echo ""
cat "$SUMMARY_FILE"
