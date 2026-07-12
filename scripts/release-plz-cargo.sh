#!/usr/bin/env bash
set -u

real_cargo="${RELEASE_PLZ_REAL_CARGO:-cargo}"

"$real_cargo" "$@"
status=$?

repo_root="$(git rev-parse --show-toplevel 2>/dev/null || true)"

if [[ -n "$repo_root" ]]; then
  git -C "$repo_root" status --porcelain=v1 -z --untracked-files=all -- ':(glob)**/Cargo.lock' |
    while IFS= read -r -d '' entry; do
      path="${entry:3}"

      if [[ "$path" == "Cargo.lock" ]]; then
        continue
      fi

      if git -C "$repo_root" ls-files --error-unmatch -- "$path" >/dev/null 2>&1; then
        git -C "$repo_root" restore --worktree -- "$path"
      else
        rm -f -- "$repo_root/$path"
      fi
    done
fi

exit "$status"
