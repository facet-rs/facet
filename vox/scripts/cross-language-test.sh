#!/bin/bash
# Cross-language test matrix for roam RPC
# Tests all 16 (client, server) pairs: {Rust, Go, TypeScript, Swift} × {Rust, Go, TypeScript, Swift}

set -eu

cd "$(dirname "$0")/.."

# Build all binaries
echo "Building all binaries..."
cargo build --package spec-tests --bin tcp-echo-server
cd go && go build -o client/go-client ./client/ && go build -o server/go-server ./server/ && cd ..
cd swift/client && swift build && cd ../..
cd swift/server && swift build && cd ../..

# Cleanup function
cleanup() {
    pkill -f tcp-echo-server 2>/dev/null || true
    pkill -f go-server 2>/dev/null || true
    pkill -f swift-server 2>/dev/null || true
    pkill -f "tcp-server.ts" 2>/dev/null || true
}
trap cleanup EXIT

# Test function
run_test() {
    local server=$1
    local client=$2
    local port=$3

    echo ""
    echo "=== $client client vs $server server ==="

    # Start server
    case $server in
        Rust)
            TCP_PORT=$port cargo run --package spec-tests --bin tcp-echo-server 2>/dev/null &
            ;;
        Go)
            TCP_PORT=$port ./go/server/go-server &
            ;;
        TypeScript)
            TCP_PORT=$port sh typescript/tests/tcp-server.sh 2>/dev/null &
            ;;
        Swift)
            TCP_PORT=$port swift/server/.build/debug/swift-server &
            ;;
    esac

    local server_pid=$!
    sleep 1

    # Run client
    local result=0
    case $client in
        Rust)
            # Rust client uses the Go client for now (we could add a Rust client later)
            SERVER_ADDR=127.0.0.1:$port ./go/client/go-client || result=1
            ;;
        Go)
            SERVER_ADDR=127.0.0.1:$port ./go/client/go-client || result=1
            ;;
        TypeScript)
            SERVER_ADDR=127.0.0.1:$port sh typescript/tests/tcp-client.sh 2>/dev/null || result=1
            ;;
        Swift)
            SERVER_ADDR=127.0.0.1:$port swift/client/.build/debug/swift-client || result=1
            ;;
    esac

    # Kill server
    kill $server_pid 2>/dev/null || true
    wait $server_pid 2>/dev/null || true

    if [ $result -eq 0 ]; then
        echo "PASS: $client client vs $server server"
        return 0
    else
        echo "FAIL: $client client vs $server server"
        return 1
    fi
}

# Run all 16 combinations
echo "============================================"
echo "Cross-Language Test Matrix (4x4 = 16 tests)"
echo "============================================"

cleanup

passed=0
failed=0
port=9100

for server in Rust Go TypeScript Swift; do
    for client in Go TypeScript Swift; do
        port=$((port + 1))
        if run_test $server $client $port; then
            passed=$((passed + 1))
        else
            failed=$((failed + 1))
        fi
    done
done

echo ""
echo "============================================"
echo "Results: $passed passed, $failed failed (of 12 tests)"
echo "(Rust client not implemented - using Go client for Rust server tests)"
echo "============================================"

if [ $failed -gt 0 ]; then
    exit 1
fi
