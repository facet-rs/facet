#!/usr/bin/env bash
# Regenerates tests/fixtures/lua/grammar.tree-sitter.json from
# tests/fixtures/lua/grammar.js using the real tree-sitter CLI.
#
# Run this after editing the vendored Lua grammar fixture, or to
# refresh the oracle after a tree-sitter-cli upgrade. Requires the
# `tree-sitter` CLI (e.g. `brew install tree-sitter`) on PATH.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

out=$(mktemp -d)
trap 'rm -rf "$out"' EXIT

tree-sitter generate --no-parser --output "$out" tests/fixtures/lua/grammar.js
cp "$out/grammar.json" tests/fixtures/lua/grammar.tree-sitter.json

echo "regenerated tests/fixtures/lua/grammar.tree-sitter.json"
