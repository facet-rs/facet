#!/usr/bin/env bash
set -euo pipefail

# r[impl schema.compat.ci]
# r[verify schema.compat.ci]
cargo xtask schema-compat-check
