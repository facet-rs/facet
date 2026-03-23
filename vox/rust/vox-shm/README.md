# vox-shm

Shared-memory transport implementation for Vox.

## Role in the Vox stack

`vox-shm` implements the `Link` layer for zero-copy local IPC using shared-memory rings.

## What this crate provides

- Host/guest shared-memory link construction
- Segment and peer orchestration helpers
- Framing and runtime integration for SHM-backed connections

## Fits with

- `shm-primitives` and `shm-primitives-async` for low-level memory/control operations
- `vox-core` for session, connection, and driver orchestration on top of SHM links

Part of the Vox workspace: <https://github.com/bearcove/vox>
