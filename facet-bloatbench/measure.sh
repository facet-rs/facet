#!/bin/bash
# Bloat measurement script for facet-bloatbench
# Usage: ./measure.sh [label]

set -e
cd "$(dirname "$0")/.."

LABEL="${1:-$(git rev-parse --short HEAD)}"
TARGET_DIR="target/bloat-measure"

echo "=== Bloat Measurement: $LABEL ==="
echo "Commit: $(git rev-parse HEAD)"
echo "Date: $(date -Iseconds)"
echo ""

# Clean and build
rm -rf "$TARGET_DIR"
echo "Building facet+json (release)..."
cargo build -p facet-bloatbench --features facet,json --release --target-dir "$TARGET_DIR" 2>&1 | grep -E "(Compiling|Finished)" | tail -3

BINARY="$TARGET_DIR/release/facet-bloatbench"

# Binary sizes
SIZE=$(stat -c%s "$BINARY")
cp "$BINARY" "${BINARY}.stripped"
strip "${BINARY}.stripped"
STRIPPED=$(stat -c%s "${BINARY}.stripped")
rm "${BINARY}.stripped"

echo ""
echo "=== Binary Sizes ==="
echo "Binary:   $SIZE bytes ($((SIZE/1024)) KB)"
echo "Stripped: $STRIPPED bytes ($((STRIPPED/1024)) KB)"

# LLVM lines (if cargo-llvm-lines is available)
if command -v cargo-llvm-lines &> /dev/null; then
    echo ""
    echo "=== LLVM Lines (building...) ==="
    LLVM_OUTPUT=$(cargo llvm-lines -p facet-bloatbench --lib --features facet,json --release --target-dir "$TARGET_DIR" 2>&1)
    TOTAL_LINE=$(echo "$LLVM_OUTPUT" | grep "(TOTAL)")
    LINES=$(echo "$TOTAL_LINE" | awk '{print $1}')
    COPIES=$(echo "$TOTAL_LINE" | awk '{print $2}')
    echo "Total lines:  $LINES"
    echo "Total copies: $COPIES"

    echo ""
    echo "=== Top 10 Contributors ==="
    echo "$LLVM_OUTPUT" | head -13 | tail -10
fi

echo ""
echo "=== Summary (copy-paste for log) ==="
echo "| Metric | Value |"
echo "|--------|-------|"
echo "| Binary size | $((SIZE/1024)) KB ($SIZE bytes) |"
echo "| Stripped size | $((STRIPPED/1024)) KB ($STRIPPED bytes) |"
if [ -n "$LINES" ]; then
    echo "| LLVM IR lines | $LINES |"
    echo "| Monomorphized copies | $COPIES |"
fi
