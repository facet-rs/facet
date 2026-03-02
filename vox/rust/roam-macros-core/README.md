# roam-macros-core

Core code generation engine for Roam service procedural macros.

## Role in the Roam stack

`roam-macros-core` is internal codegen infrastructure behind the service-definition layer.

## What this crate provides

- Macro expansion logic shared by public proc-macro entry points
- Token generation for clients, dispatchers, and service detail artifacts

## Fits with

- `roam-service-macros` (public proc-macro crate)
- `roam-macros-parse` (grammar/parser front-end)

Part of the Roam workspace: <https://github.com/bearcove/roam>
