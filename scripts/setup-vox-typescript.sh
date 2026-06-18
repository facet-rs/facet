#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd -- "$script_dir/.." && pwd)"
vox_dir="$repo_root/vox"
subject_generated_pkg="$vox_dir/typescript/subject/node_modules/@bearcove/vox-generated/package.json"
target_dir="${CARGO_TARGET_DIR:-$repo_root/target}"
subject_rust_bin="$target_dir/debug/subject-rust"
pnpm_version="${VOX_PNPM_VERSION:-11.7.0}"

if [ ! -x "$subject_rust_bin" ]; then
    cargo build -p subject-rust
fi

if [ ! -f "$subject_generated_pkg" ]; then
    pnpm_cmd="${PNPM:-}"

    if [ -z "$pnpm_cmd" ] && command -v pnpm >/dev/null 2>&1; then
        pnpm_cmd="$(command -v pnpm)"
    fi

    if [ -z "$pnpm_cmd" ] && command -v corepack >/dev/null 2>&1; then
        if corepack prepare "pnpm@$pnpm_version" --activate; then
            pnpm_cmd="$(command -v pnpm || true)"
        fi
    fi

    if [ -z "$pnpm_cmd" ] && command -v npm >/dev/null 2>&1; then
        pnpm_root="$target_dir/vox-pnpm"
        pnpm_cmd="$pnpm_root/node_modules/.bin/pnpm"
        if [ ! -x "$pnpm_cmd" ]; then
            npm install --prefix "$pnpm_root" "pnpm@$pnpm_version"
        fi
    fi

    if [ -z "$pnpm_cmd" ] || { ! command -v "$pnpm_cmd" >/dev/null 2>&1 && [ ! -x "$pnpm_cmd" ]; }; then
        echo "setup-vox-typescript: pnpm is required; install pnpm, corepack, or npm" >&2
        exit 127
    fi

    "$pnpm_cmd" --dir "$vox_dir" install --frozen-lockfile
fi
