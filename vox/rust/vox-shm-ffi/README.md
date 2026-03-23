# vox-shm-ffi

C-compatible bindings for Vox shared-memory primitives.

## Role in the Vox stack

`vox-shm-ffi` exposes SHM building blocks at the FFI boundary so non-Rust runtimes can interoperate.

## What this crate provides

- C ABI wrappers around shared-memory primitive operations
- Headers/artifacts for embedding SHM support in foreign runtimes

## Fits with

- `shm-primitives` and `vox-shm` internals
- Swift and other non-Rust integrations that need SHM access

Part of the Vox workspace: <https://github.com/bearcove/vox>
