#!/bin/bash
set -e

# Integration test script for rapace-typescript
# This script:
# 1. Starts the rapace browser-tests-server
# 2. Waits for it to be ready
# 3. Runs the integration tests
# 4. Cleans up the server

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
RAPACE_DIR="${RAPACE_DIR:-$PROJECT_DIR/../rapace}"

PORT="${RAPACE_BROWSER_WS_PORT:-4788}"
SERVER_PID=""

cleanup() {
    if [ -n "$SERVER_PID" ]; then
        echo "Stopping server (PID: $SERVER_PID)..."
        kill "$SERVER_PID" 2>/dev/null || true
        wait "$SERVER_PID" 2>/dev/null || true
    fi
}

trap cleanup EXIT

# Check if rapace directory exists
if [ ! -d "$RAPACE_DIR" ]; then
    echo "Error: rapace directory not found at $RAPACE_DIR"
    echo "Set RAPACE_DIR environment variable to the rapace repo path"
    exit 1
fi

echo "Building rapace-browser-tests-server..."
(cd "$RAPACE_DIR" && cargo build -p rapace-browser-tests-server --release)

echo "Starting server on port $PORT..."
RAPACE_BROWSER_WS_PORT="$PORT" "$RAPACE_DIR/target/release/rapace-browser-tests-server" &
SERVER_PID=$!

# Wait for server to be ready
echo "Waiting for server to be ready..."
MAX_ATTEMPTS=30
ATTEMPT=0
while ! nc -z 127.0.0.1 "$PORT" 2>/dev/null; do
    ATTEMPT=$((ATTEMPT + 1))
    if [ "$ATTEMPT" -ge "$MAX_ATTEMPTS" ]; then
        echo "Error: Server failed to start after $MAX_ATTEMPTS attempts"
        exit 1
    fi
    sleep 0.2
done
echo "Server is ready!"

# Run integration tests
echo "Running integration tests..."
cd "$PROJECT_DIR"
RAPACE_BROWSER_WS_PORT="$PORT" npm test -- --test-name-pattern="Integration"

echo "Integration tests passed!"
