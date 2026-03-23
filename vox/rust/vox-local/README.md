# vox-local

Cross-platform local IPC transport utilities for Vox.

## Role in the Vox stack

`vox-local` helps construct `Link`-layer connections for same-host communication.

## What this crate provides

- Unix domain socket support on Unix targets
- Named pipe support on Windows targets
- Local transport setup used by higher-level stream integration

## Fits with

- `vox-stream` framing and link adaptation
- `vox-core` session establishment and driver runtime

Part of the Vox workspace: <https://github.com/bearcove/vox>
