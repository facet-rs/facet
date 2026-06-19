#!/usr/bin/env bash
set -u

real_cargo="${RELEASE_PLZ_REAL_CARGO:-cargo}"

"$real_cargo" "$@"
status=$?

if [[ "${1:-}" == "package" ]]; then
  repo_root="$(git rev-parse --show-toplevel 2>/dev/null || true)"
  lockfile="$repo_root/phon/rust/Cargo.lock"

  if [[ -n "$repo_root" && -e "$lockfile" ]]; then
    if git -C "$repo_root" ls-files --error-unmatch phon/rust/Cargo.lock >/dev/null 2>&1; then
      git -C "$repo_root" checkout -- phon/rust/Cargo.lock
    else
      rm -f "$lockfile"
    fi
  fi
fi

exit "$status"
