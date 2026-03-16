#!/usr/bin/env bash
set -euo pipefail

CANONICAL="docs/readme-footer.md"
MODE="${1:-check}"

if [[ ! -f "$CANONICAL" ]]; then
    echo "error: canonical file '$CANONICAL' not found" >&2
    exit 1
fi

# Find all crate copies (exclude the canonical one)
copies=()
while IFS= read -r -d '' f; do
    copies+=("$f")
done < <(find . -name readme-footer.md -not -path "./$CANONICAL" -print0 | sort -z)

if [[ ${#copies[@]} -eq 0 ]]; then
    echo "warning: no readme-footer.md copies found" >&2
    exit 0
fi

case "$MODE" in
    check)
        bad=()
        for f in "${copies[@]}"; do
            if ! diff -q "$CANONICAL" "$f" > /dev/null 2>&1; then
                bad+=("$f")
            fi
        done

        if [[ ${#bad[@]} -gt 0 ]]; then
            echo "error: the following readme-footer.md files are out of sync with $CANONICAL:" >&2
            for f in "${bad[@]}"; do
                echo "  $f" >&2
            done
            echo "" >&2
            echo "run 'just sync-readme-footer' to fix" >&2
            exit 1
        fi

        echo "ok: all ${#copies[@]} readme-footer.md copies are in sync"
        ;;
    sync)
        for f in "${copies[@]}"; do
            if ! diff -q "$CANONICAL" "$f" > /dev/null 2>&1; then
                cp "$CANONICAL" "$f"
                echo "updated $f"
            fi
        done
        echo "done: all ${#copies[@]} readme-footer.md copies are in sync"
        ;;
    *)
        echo "usage: $0 [check|sync]" >&2
        exit 1
        ;;
esac
