# shm-primitives

Lock-free shared-memory data structures and peer coordination primitives.

## Role in the Vox stack

`shm-primitives` is foundational infrastructure below the `Link` layer for SHM transports.

## What this crate provides

- Ring/buffer and slot-management primitives for shared-memory IPC
- Segment and peer-state building blocks used by higher-level SHM transport code

## Fits with

- `vox-shm` transport implementation
- `vox-shm-ffi` for foreign-runtime interoperability
- `shm-primitives-async` for async OS control paths

Part of the Vox workspace: <https://github.com/bearcove/vox>
