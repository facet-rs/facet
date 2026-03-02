# roam-hash

Method and schema hashing utilities used for stable RPC identity.

## Role in the Roam stack

`roam-hash` supports the RPC surface by producing deterministic identifiers used by generated code and runtime dispatch.

## What this crate provides

- Stable hashing for methods and service-level identifiers
- Utilities used by macros and code generators to align call IDs

## Fits with

- `roam-service-macros` and `roam-codegen`
- `roam` and `roam-core` dispatch/runtime behavior

Part of the Roam workspace: <https://github.com/bearcove/roam>
