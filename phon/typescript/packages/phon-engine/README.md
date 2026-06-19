# @bearcove/phon-engine

TypeScript engine for Phon compact encoding, compatibility planning, interpreter execution, and optional JIT compilation.

## Role in the Phon stack

`@bearcove/phon-engine` sits above `@bearcove/phon-schema`: it takes writer and reader schemas, builds compatibility plans, and executes those plans against compact Phon bytes.

## What this package provides

- Compact schema-driven encode/decode helpers
- Writer-schema to reader-schema compatibility planning
- Interpreter execution for compatibility plans
- `new Function` JIT compilation with interpreter fallback
- Typed encode/decode helpers for generated TypeScript bindings

## Fits with

- `@bearcove/phon-schema` for schema and Value definitions plus wire primitives
- `@bearcove/phon` as the public front door for generated consumers
- Vox TypeScript packages that need schema-aware binary serialization

Part of the Facet workspace: <https://github.com/facet-rs/facet>
