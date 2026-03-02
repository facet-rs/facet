# roam-shm

Shared-memory transport implementation for Roam.

## Role in the Roam stack

`roam-shm` implements the `Link` layer for zero-copy local IPC using shared-memory rings.

## What this crate provides

- Host/guest shared-memory link construction
- Segment and peer orchestration helpers
- Framing and runtime integration for SHM-backed connections

## Fits with

- `shm-primitives` and `shm-primitives-async` for low-level memory/control operations
- `roam-core` for session, connection, and driver orchestration on top of SHM links

Part of the Roam workspace: <https://github.com/bearcove/roam>
