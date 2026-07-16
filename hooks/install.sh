#!/usr/bin/env bash
# Point git at the repo's tracked hooks/ directory instead of copying scripts
# into each .git/hooks. `core.hooksPath` is stored in the shared git config, so
# a single run covers the main checkout and every linked worktree, and edits to
# the tracked hooks take effect immediately with no re-install.
#
# The hooks themselves run `capn`, which the nix dev shell provides (flake.nix).
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

chmod +x hooks/pre-commit hooks/pre-push

git config core.hooksPath hooks

echo "✔ core.hooksPath set to 'hooks' (covers all worktrees)"
if ! command -v capn >/dev/null 2>&1; then
  echo "  note: capn is not on PATH yet — enter the nix dev shell to get it."
fi
