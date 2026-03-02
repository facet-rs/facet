# roam-local

Cross-platform local IPC transport utilities for Roam.

## Role in the Roam stack

`roam-local` helps construct `Link`-layer connections for same-host communication.

## What this crate provides

- Unix domain socket support on Unix targets
- Named pipe support on Windows targets
- Local transport setup used by higher-level stream integration

## Fits with

- `roam-stream` framing and link adaptation
- `roam-core` session establishment and driver runtime

Part of the Roam workspace: <https://github.com/bearcove/roam>
