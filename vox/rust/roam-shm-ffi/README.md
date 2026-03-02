# roam-shm-ffi

C-compatible bindings for Roam shared-memory primitives.

## Role in the Roam stack

`roam-shm-ffi` exposes SHM building blocks at the FFI boundary so non-Rust runtimes can interoperate.

## What this crate provides

- C ABI wrappers around shared-memory primitive operations
- Headers/artifacts for embedding SHM support in foreign runtimes

## Fits with

- `shm-primitives` and `roam-shm` internals
- Swift and other non-Rust integrations that need SHM access

Part of the Roam workspace: <https://github.com/bearcove/roam>
