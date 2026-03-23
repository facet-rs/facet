# vox-fdpass

Cross-platform descriptor/handle passing primitives for local IPC scenarios.

## Role in the Vox stack

`vox-fdpass` is a low-level transport helper used below `Link` setup for passing OS resources between peers.

## What this crate provides

- Unix FD passing support
- Windows handle passing support
- Utilities to integrate descriptor passing into local transport bootstrapping

## Fits with

- `vox-local` and stream-based local connection setup
- `vox-shm` bootstrap paths that may require OS resource transfer

Part of the Vox workspace: <https://github.com/bearcove/vox>
