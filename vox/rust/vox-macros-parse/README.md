# vox-macros-parse

Parser and grammar utilities for Vox service macro inputs.

## Role in the Vox stack

`vox-macros-parse` supports the compile-time schema layer by parsing service trait definitions.

## What this crate provides

- Parser structures for service-trait syntax and macro input handling
- Intermediate representations consumed by macro codegen

## Fits with

- `vox-macros-core` for expansion/token generation
- `vox-service-macros` as the public macro crate

Part of the Vox workspace: <https://github.com/bearcove/vox>
