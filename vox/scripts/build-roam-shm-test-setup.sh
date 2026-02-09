#!/usr/bin/env bash
set -euo pipefail

cargo build -p roam-shm --bin guest_process --all-features
swift build --package-path swift/roam-runtime --product shm-bootstrap-client
