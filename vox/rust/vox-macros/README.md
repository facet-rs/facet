# vox-service-macros

Procedural macros for generating Vox service clients and dispatchers.

## Role in the Vox stack

`vox-service-macros` powers the schema/source-of-truth layer where Rust traits define service contracts.

## What this crate provides

- `#[vox::service]` expansion support
- Generated service trait plumbing, client stubs, and dispatcher glue

## Fits with

- `vox` as the public API surface
- `vox-macros-core` and `vox-macros-parse` for expansion internals
- `vox-hash` for stable method identity

Part of the Vox workspace: <https://github.com/bearcove/vox>
