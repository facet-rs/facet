#!/usr/bin/env bash
set -euo pipefail

HOOK_SOURCE_DIR="$(git rev-parse --show-toplevel)/scripts"
GIT_DIR="$(git rev-parse --git-dir)"

copy_hook() {
  local src="$1"
  local dst="$2"

  mkdir -p "$(dirname "$dst")"
  cp "$src" "$dst"
  chmod +x "$dst"

  echo "✔ installed $(basename "$src") → $dst"
}

install_for_dir() {
  local hook_dir="$1"

  for hook in "$HOOK_SOURCE_DIR"/pre-*; do
    local name
    name="$(basename "$hook")"
    local target="$hook_dir/$name"

    copy_hook "$hook" "$target"
  done
}

echo "Installing hooks from $HOOK_SOURCE_DIR …"

# main repo
install_for_dir "$GIT_DIR/hooks"

# worktrees
for wt in "$GIT_DIR"/worktrees/*; do
  if [ -d "$wt" ]; then
    install_for_dir "$wt/hooks"
  fi
done

echo "All hooks installed successfully."
