#!/bin/bash
# Run the Twitter JIT test repeatedly until it crashes, then get backtrace

TEST_BIN="target/debug/deps/generated_benchmark_tests-900ea4ff8d0cdcc6"

for i in $(seq 1 50); do
    echo "=== Run $i ==="
    output=$(MallocScribble=1 "$TEST_BIN" test_twitter::test_facet_format_jit_t1_deserialize 2>&1)
    result=$?
    if [ $result -ne 0 ]; then
        echo "CRASHED with exit code $result"
        echo "$output"
        # Now run under lldb to get backtrace
        echo ""
        echo "=== Getting backtrace with lldb ==="
        MallocScribble=1 lldb -b \
            -o "run test_twitter::test_facet_format_jit_t1_deserialize" \
            -o "bt all" \
            -o "register read" \
            "$TEST_BIN"
        exit 1
    else
        echo "PASSED"
    fi
done

echo "All 50 runs passed!"
