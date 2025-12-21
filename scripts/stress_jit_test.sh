#!/bin/bash
# Stress test the flaky JIT test until it crashes
# Usage: ./scripts/stress_jit_test.sh [iterations]

BINARY="target/debug/deps/facet_format_suite-ac9ac5c058233617"
TEST="test_twitter::test_facet_format_jit_t1_deserialize"
MAX_RUNS="${1:-100}"

# Rebuild first
cargo test --package facet-format-suite --no-run 2>/dev/null

if [ ! -f "$BINARY" ]; then
    echo "Binary not found. Run: cargo test --package facet-format-suite --no-run"
    exit 1
fi

echo "Running $TEST up to $MAX_RUNS times..."
echo "Press Ctrl+C to stop"
echo ""

for i in $(seq 1 $MAX_RUNS); do
    if ! "$BINARY" "$TEST" --exact 2>&1 > /tmp/jit_test_output.txt; then
        echo ""
        echo "=== CRASHED on run $i ==="
        cat /tmp/jit_test_output.txt
        echo ""
        echo "To debug: lldb -s .lldb_jit_debug"
        exit 1
    fi
    printf "\rPass %d/%d" "$i" "$MAX_RUNS"
done

echo ""
echo "All $MAX_RUNS runs passed!"
