# roam-types

Protocol and runtime data model shared across Roam implementations.

## Role in the Roam stack

`roam-types` spans the `Requests / Channels`, `Connections`, and `Session` layers by defining shared message and control types.

## What this crate provides

- Wire-level and runtime-facing enums/structs used by the protocol
- Request/response and channel-related types
- Common error and metadata types consumed by runtime and transports

## Fits with

- `roam`, `roam-core`, and transport crates (`roam-stream`, `roam-websocket`, `roam-shm`)
- `roam-codegen` when generating non-Rust bindings

Part of the Roam workspace: <https://github.com/bearcove/roam>
