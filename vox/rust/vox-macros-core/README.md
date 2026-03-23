# vox-macros-core

Core code generation engine for Vox service procedural macros.

## Role in the Vox stack

`vox-macros-core` is internal codegen infrastructure behind the service-definition layer.

## What this crate provides

- Macro expansion logic shared by public proc-macro entry points
- Token generation for clients, dispatchers, and service detail artifacts

## Fits with

- `vox-service-macros` (public proc-macro crate)
- `vox-macros-parse` (grammar/parser front-end)

Part of the Vox workspace: <https://github.com/bearcove/vox>
