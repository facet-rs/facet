#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
BINARY="$SCRIPT_DIR/.build/release/subject-swift"
RUNTIME_DIR="$SCRIPT_DIR/../vox-runtime"

if [ "${SUBJECT_SWIFT_PRINT_BIN_PATH:-0}" = "1" ] || [ "${1:-}" = "--print-bin-path" ]; then
  printf '%s\n' "$BINARY"
  exit 0
fi

cd "$SCRIPT_DIR"

needs_build=0
if [ ! -x "$BINARY" ]; then
  needs_build=1
else
  newer=$(find \
    "$SCRIPT_DIR/Package.swift" \
    "$SCRIPT_DIR/Package.resolved" \
    "$SCRIPT_DIR/Sources" \
    "$RUNTIME_DIR/Package.swift" \
    "$RUNTIME_DIR/Package.resolved" \
    "$RUNTIME_DIR/Sources" \
    -newer "$BINARY" \
    -print \
    -quit)
  if [ -n "$newer" ]; then
    needs_build=1
  fi
fi

if [ "$needs_build" = "1" ]; then
  swift build -c release --product subject-swift
fi

exec "$BINARY" "$@"
