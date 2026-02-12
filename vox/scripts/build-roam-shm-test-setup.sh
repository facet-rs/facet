#!/usr/bin/env bash
set -euo pipefail

cargo build -p roam-shm --bin guest_process --all-features

# Skip Swift builds on Linux (Swift is only available on macOS)
if [[ "$OSTYPE" == "darwin"* ]]; then
  swift build --package-path swift/roam-runtime --product shm-bootstrap-client
  swift build --package-path swift/roam-runtime --product shm-guest-client
  swift build -c release --package-path swift/subject --product subject-swift
else
  echo "Skipping Swift builds on non-macOS platform ($OSTYPE)"
fi
