#!/bin/bash
set -euo pipefail

REGISTRY_DIR="$HOME/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f"
SAMPLE_SIZE=200

echo "Finding all Cargo.toml files..."
ALL_FILES=$(find "$REGISTRY_DIR" -name "Cargo.toml" -type f)
TOTAL=$(echo "$ALL_FILES" | wc -l | tr -d ' ')

echo "Total files: $TOTAL"
echo "Sampling $SAMPLE_SIZE random files..."

SAMPLED=$(echo "$ALL_FILES" | shuf -n "$SAMPLE_SIZE")

SUCCESS=0
FAIL=0
FAILURES=()

while IFS= read -r file; do
    CRATE_NAME=$(basename $(dirname "$file"))
    if cargo run --quiet --example test_parse -- "$file" 2>/dev/null; then
        ((SUCCESS++))
    else
        ((FAIL++))
        FAILURES+=("$file")
    fi
done <<< "$SAMPLED"

echo ""
echo "========================================="
echo "Results: $SUCCESS passed, $FAIL failed"
echo "Success rate: $(awk "BEGIN {printf \"%.1f\", ($SUCCESS/$SAMPLE_SIZE)*100}")%"
echo "========================================="

if [ $FAIL -gt 0 ]; then
    echo ""
    echo "Failed crates:"
    for file in "${FAILURES[@]}"; do
        CRATE_NAME=$(basename $(dirname "$file"))
        echo "  - $CRATE_NAME"
    done
fi
