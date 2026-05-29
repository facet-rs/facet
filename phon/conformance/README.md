# Conformance corpus

The cross-language oracle. phon's correctness rests on every implementation —
Rust, Swift, TypeScript — producing **identical bytes** and computing
**identical `SchemaId`s** from the same input. This corpus is how that's checked
mechanically, so a divergent BLAKE3, an endianness slip, or an off-by-one in a
length prefix is caught the moment it appears rather than in production drift.

## How it works

Rust is the source of truth. The generator (`rust/phon-conformance`) builds a
fixed set of schemas and sample values, encodes each value in both wire modes,
computes the schema identities, and writes the result under `cases/`. Every
implementation then **loads** the corpus and checks itself against it:

- Rust: regenerates in-memory and asserts it matches what's committed (so a
  change to the committed corpus is a reviewed event, never silent), and
  round-trips every value.
- Swift: `swift/phon/Tests/PhonTests` loads `cases/` and verifies it encodes the
  same compact bytes and computes the same `SchemaId` as Rust.
- TypeScript: a `vitest` suite under `typescript/tests/conformance` does the
  same.

The corpus is committed to the repo, so a reviewer sees the wire bytes change in
a diff. Regenerating is `cargo run -p phon-conformance` (it overwrites `cases/`);
a clean working tree afterward means nothing drifted.

## Layout

```
conformance/
  cases/
    <case-name>/
      manifest.json     generated via facet-json (never hand-written):
                        the case's schema names, their expected SchemaIds
                        (hex), and the per-value file list
      schemas.phon      self-describing bytes of the transitive schema closure
      <value>.selfdesc  a sample value in self-describing mode
      <value>.compact   the same value in compact mode, against its schema
```

A case exercises one root schema and its closure. The schema set deliberately
covers the corners that diverge across languages: recursive schemas (the
SCC/back-reference identity walk), generics, every primitive width, `Array` vs
`Tensor`, `Option` nesting, enums with each payload shape, alignment padding
before wide elements, `Dynamic`, and `External` metadata.

## Why bytes, not a portable phon file

The corpus can't be stored *as* compact phon, because the codec is the thing
under test — that would be circular. So cases are raw byte files plus a
`manifest.json` for the human-readable expectations. The manifest is emitted by
`facet-json`, never assembled by hand.
