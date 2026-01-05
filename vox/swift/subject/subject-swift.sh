#!/bin/sh
set -eu

BIN="./swift/subject/.build/debug/subject-swift"
if [ ! -x "$BIN" ]; then
  echo "subject-swift: missing $BIN" >&2
  echo "subject-swift: build it with: swift build --package-path swift/subject" >&2
  exit 1
fi

exec "$BIN"

