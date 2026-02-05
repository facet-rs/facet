#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
cd "$SCRIPT_DIR"

# Run the subject (assumes pre-built with: swift build -c release)
exec ./\.build/release/subject-swift
