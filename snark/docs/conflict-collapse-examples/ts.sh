#!/bin/bash
# ts.sh <grammar.js> <input>...   — generate + parse each input with tree-sitter
g="$1"; shift
d=$(mktemp -d); cp "$g" "$d/grammar.js"
( cd "$d" && tree-sitter generate >/dev/null 2>gen.err ) || { echo "  GENERATE FAILED:"; sed 's/^/    /' "$d/gen.err" | head -5; rm -rf "$d"; exit 0; }
for inp in "$@"; do
  printf '%s' "$inp" > "$d/in.txt"
  out=$( cd "$d" && tree-sitter parse in.txt 2>&1 | tr '\n' ' ' | sed 's/\[[0-9, ]*\] - \[[0-9, ]*\]//g; s/  */ /g' )
  printf '  %-11s -> %s\n' "$inp" "$out"
done
rm -rf "$d"
