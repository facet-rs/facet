#!/usr/bin/env bash
# Shared environment setup for miri and llvm-cov targets
# Sources this file to get the correct nightly toolchain installed and configured

export RUSTUP_TOOLCHAIN=nightly-2026-01-28
export MIRIFLAGS="-Zmiri-strict-provenance -Zmiri-env-forward=NEXTEST"

rustup toolchain install "${RUSTUP_TOOLCHAIN}"
rustup "+${RUSTUP_TOOLCHAIN}" component add miri rust-src llvm-tools-preview
