#!/bin/bash
# Stress test JIT tests - run in a loop until crash, then debug with lldb
# Usage: ./scripts/stress-test-jit.sh [iterations] [test-filter]

set -e

ITERATIONS=${1:-1000}
FILTER=${2:-'test(jit_t1)'}
COUNT=0

# Enable malloc debugging
export MallocScribble=1           # Fill allocated/freed memory with 0xAA/0x55

# Enable crash handler (pauses for lldb attach on crash)
export FACET_JIT_CRASH_HANDLER=1

echo "Running $FILTER up to $ITERATIONS times..."
echo "Malloc debugging enabled (MallocScribble, MallocGuardEdges)"
echo "Press Ctrl+C to stop"
echo ""

while [ $COUNT -lt $ITERATIONS ]; do
    COUNT=$((COUNT + 1))
    printf "\r[%4d/%d] Running tests..." "$COUNT" "$ITERATIONS"

    # Run tests normally (fast) - capture output
    if ! OUTPUT=$(cargo nextest run -E "$FILTER" --no-fail-fast 2>&1); then
        # Test crashed! Show the failure
        echo ""
        echo ""
        echo "╔════════════════════════════════════════════════════════════╗"
        echo "║  CRASH DETECTED on iteration $COUNT                        "
        echo "╚════════════════════════════════════════════════════════════╝"
        echo ""
        echo "$OUTPUT"
        echo ""

        # Extract the failed test name
        FAILED_TEST=$(echo "$OUTPUT" | grep -oE "FAIL.*test_[a-z_]+::test_facet_format_jit_t1_deserialize" | head -1 | awk '{print $NF}')

        if [ -n "$FAILED_TEST" ]; then
            echo "Re-running failed test under lldb: $FAILED_TEST"
            echo ""
            cargo nextest run --profile lldb -E "test($FAILED_TEST)" --no-capture
        fi

        exit 1
    fi
done

echo ""
echo "✓ All $ITERATIONS iterations passed!"
