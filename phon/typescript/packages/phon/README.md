# @bearcove/phon

TypeScript front door for Phon-generated bindings.

## Role in the Phon stack

`@bearcove/phon` is the package generated TypeScript peers should depend on when they need the stable public Phon entrypoint.

## What this package provides

- The top-level TypeScript package boundary for Phon consumers
- Re-exports for the schema and engine package markers used by generated bindings
- A stable import target for code generated from Facet-derived Phon schemas

## Fits with

- `@bearcove/phon-schema` for schema models, schema identity, dynamic values, and self-describing bytes
- `@bearcove/phon-engine` for compact schema-driven encode/decode, compatibility planning, and JIT/interpreter execution
- Vox TypeScript packages that use Phon for wire serialization

Part of the Facet workspace: <https://github.com/facet-rs/facet>
