# @bearcove/phon-schema

TypeScript schema, value, and self-describing wire primitives for Phon.

## Role in the Phon stack

`@bearcove/phon-schema` defines the TypeScript representation of Phon schemas and dynamic values. It is the shared contract used by the compact engine, codegen output, and conformance tests.

## What this package provides

- Phon schema model types and schema registry helpers
- Dynamic `Value` encode/decode support
- Self-describing schema and value parsing
- Wire primitives such as readers, byte sinks, tags, and hex helpers
- Schema validation and alignment/minimum-size analysis

## Fits with

- `@bearcove/phon-engine` for compact encode/decode and compatibility planning
- `@bearcove/phon` as the public front door for generated consumers
- Vox TypeScript packages that share schema-derived wire contracts

Part of the Facet workspace: <https://github.com/facet-rs/facet>
