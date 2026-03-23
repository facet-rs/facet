# shm-primitives-async

Async operating-system control helpers for shared-memory transport plumbing.

## Role in the Vox stack

`shm-primitives-async` supports low-level SHM transport setup and coordination below the `Link` layer.

## What this crate provides

- Async doorbell and mmap-control operations
- OS-specific async control paths required by SHM host/guest coordination

## Fits with

- `shm-primitives` core data structures
- `vox-shm` transport orchestration

Part of the Vox workspace: <https://github.com/bearcove/vox>
