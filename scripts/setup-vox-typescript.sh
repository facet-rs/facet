#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd -- "$script_dir/.." && pwd)"
vox_dir="$repo_root/vox"
subject_generated_pkg="$vox_dir/typescript/subject/node_modules/@bearcove/vox-generated/package.json"
target_dir="${CARGO_TARGET_DIR:-$repo_root/target}"
subject_rust_bin="$target_dir/debug/subject-rust"

if [ ! -x "$subject_rust_bin" ]; then
    cargo build -p subject-rust
fi

if [ ! -f "$subject_generated_pkg" ]; then
    if ! command -v pnpm >/dev/null 2>&1; then
        if command -v corepack >/dev/null 2>&1; then
            corepack enable pnpm
        fi
    fi

    if ! command -v pnpm >/dev/null 2>&1; then
        echo "setup-vox-typescript: pnpm is required to run Vox TypeScript spec subjects" >&2
        exit 127
    fi

    pnpm --dir "$vox_dir" install --frozen-lockfile
fi
