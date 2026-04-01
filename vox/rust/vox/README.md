# vox

High-level Rust API for defining, implementing, and consuming Vox services.

## Role in the Vox stack

`vox` sits at the RPC surface (`Requests / Channels`) and exposes the developer-facing service model.

## What this crate provides

- `#[vox::service]`-driven service definitions and generated clients/dispatchers
- Core RPC traits and types re-exported for app-level use
- Feature-gated transport facade via `vox::transport`:
  - `transport-tcp` -> `vox::transport::tcp`
  - `transport-local` -> `vox::transport::local`
  - `transport-shm` -> `vox::transport::shm`
- Integration point for the rest of the Rust runtime crates

## Fits with

- `vox-core` for session/driver/runtime internals
- `vox-types` for protocol data model
- `vox-service-macros` for code generation from service traits

Part of the Vox workspace: <https://github.com/bearcove/vox>
