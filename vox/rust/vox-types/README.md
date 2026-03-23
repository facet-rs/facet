# vox-types

Protocol and runtime data model shared across Vox implementations.

## Role in the Vox stack

`vox-types` spans the `Requests / Channels`, `Connections`, and `Session` layers by defining shared message and control types.

## What this crate provides

- Wire-level and runtime-facing enums/structs used by the protocol
- Request/response and channel-related types
- Common error and metadata types consumed by runtime and transports

## Fits with

- `vox`, `vox-core`, and transport crates (`vox-stream`, `vox-websocket`, `vox-shm`)
- `vox-codegen` when generating non-Rust bindings

Part of the Vox workspace: <https://github.com/bearcove/vox>
