# facet-macros-impl

Implementation of facet derive macros, combining parsing and code generation.

This crate provides the internal implementation for `#[derive(Facet)]` and related procedural macros. It's used by `facet-macros` (the proc-macro crate) and should not be used directly.

## Features

- `function` - Enable function signature parsing and code generation for `#[facet_fn]`

## Structure

This crate merges what was previously two separate crates:
- Parsing infrastructure (type definitions, attribute parsing)
- Code emission (derive macro implementation, extension handling)
