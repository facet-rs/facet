# roam-fdpass

Cross-platform descriptor/handle passing primitives for local IPC scenarios.

## Role in the Roam stack

`roam-fdpass` is a low-level transport helper used below `Link` setup for passing OS resources between peers.

## What this crate provides

- Unix FD passing support
- Windows handle passing support
- Utilities to integrate descriptor passing into local transport bootstrapping

## Fits with

- `roam-local` and stream-based local connection setup
- `roam-shm` bootstrap paths that may require OS resource transfer

Part of the Roam workspace: <https://github.com/bearcove/roam>
