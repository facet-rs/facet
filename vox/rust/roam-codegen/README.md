# roam-codegen

Language binding generator for Roam service descriptors.

## Role in the Roam stack

`roam-codegen` bridges Rust-defined schemas to non-Rust clients/servers above the RPC surface.

## What this crate provides

- TypeScript and Swift code generation targets
- Rendering of service descriptors into client/server scaffolding

## Fits with

- `roam` service definitions and generated descriptors
- `roam-hash` and `roam-types` for shared protocol identity and type model

Part of the Roam workspace: <https://github.com/bearcove/roam>
