#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

HOOKS_SOURCE="$PROJECT_ROOT/.cargo-husky/hooks"

GIT_METADATA="$PROJECT_ROOT/.git"
if [ -d "$GIT_METADATA" ]; then
    GIT_DIR="$GIT_METADATA"
elif [ -f "$GIT_METADATA" ]; then
    GITDIR_LINE="$(sed -n 's/^gitdir: //p' "$GIT_METADATA")"
    if [ -z "$GITDIR_LINE" ]; then
        echo "Error: Unable to determine git dir from $GIT_METADATA"
        exit 1
    fi
    if [[ "$GITDIR_LINE" = /* ]]; then
        GIT_DIR="$GITDIR_LINE"
    else
        GIT_DIR="$(cd "$PROJECT_ROOT/$GITDIR_LINE" && pwd)"
    fi
else
    echo "Error: $PROJECT_ROOT does not appear to be a git repository"
    exit 1
fi

HOOKS_DEST="$GIT_DIR/hooks"

# Check if source directory exists
if [ ! -d "$HOOKS_SOURCE" ]; then
    echo "Error: Source hooks directory not found: $HOOKS_SOURCE"
    exit 1
fi

# Ensure .git/hooks directory exists
if [ ! -d "$HOOKS_DEST" ]; then
    echo "Git hooks directory not found, creating: $HOOKS_DEST"
    mkdir -p "$HOOKS_DEST"
fi

# Copy hooks
echo "Copying hooks from $HOOKS_SOURCE to $HOOKS_DEST..."
cp -v "$HOOKS_SOURCE"/* "$HOOKS_DEST/"

# Make hooks executable
chmod -v +x "$HOOKS_DEST"/*

echo "✓ Hooks installed successfully"
