# roam-service-macros

Procedural macros for generating Roam service clients and dispatchers.

## Role in the Roam stack

`roam-service-macros` powers the schema/source-of-truth layer where Rust traits define service contracts.

## What this crate provides

- `#[roam::service]` expansion support
- Generated service trait plumbing, client stubs, and dispatcher glue

## Fits with

- `roam` as the public API surface
- `roam-macros-core` and `roam-macros-parse` for expansion internals
- `roam-hash` for stable method identity

Part of the Roam workspace: <https://github.com/bearcove/roam>
