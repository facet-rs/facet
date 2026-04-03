#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
BINARY="$SCRIPT_DIR/.build/release/subject-swift"

if [ "${SUBJECT_SWIFT_PRINT_BIN_PATH:-0}" = "1" ] || [ "${1:-}" = "--print-bin-path" ]; then
  printf '%s\n' "$BINARY"
  exit 0
fi

cd "$SCRIPT_DIR"

# Run the subject (assumes pre-built with: swift build -c release)
exec "$BINARY" "$@"
