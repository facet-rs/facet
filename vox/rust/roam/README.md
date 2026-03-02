# roam

High-level Rust API for defining, implementing, and consuming Roam services.

## Role in the Roam stack

`roam` sits at the RPC surface (`Requests / Channels`) and exposes the developer-facing service model.

## What this crate provides

- `#[roam::service]`-driven service definitions and generated clients/dispatchers
- Core RPC traits and types re-exported for app-level use
- Integration point for the rest of the Rust runtime crates

## Fits with

- `roam-core` for session/driver/runtime internals
- `roam-types` for protocol data model
- `roam-service-macros` for code generation from service traits

Part of the Roam workspace: <https://github.com/bearcove/roam>
