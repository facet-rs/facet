# Conformance corpus

The cross-language oracle. phon's correctness rests on every implementation —
Rust, Swift, TypeScript — agreeing **byte for byte** on the wire and computing
**identical `SchemaId`s** from the same input. This corpus is how that's checked
mechanically, so a divergent BLAKE3, an endianness slip, or an off-by-one in a
length prefix is caught the moment it appears.

## How it works

Rust is the source of truth. The generator (`rust/phon-conformance`) builds a
fixed set of schema cases, resolves their ids, and writes each schema as
self-describing bytes. **The expected `SchemaId` is baked into those bytes** (a
schema carries its own id), so there's no separate manifest of expected values —
the committed `.phon` files *are* the golden.

Every implementation then loads the corpus and checks, for each schema:

- **round-trip** — the bytes decode to a schema and re-encode to the same bytes
  (its encoder and decoder match Rust's);
- **identity** — recomputing the schema's id from the decoded batch, using *its
  own* identity hash, reproduces the id baked into the bytes (its BLAKE3 and
  canonical encoding match Rust's).

A change to the codec, the identity algorithm, or a case definition changes the
committed bytes — visible in the git diff and reviewed. Regenerate with
`cargo run -p phon-conformance`; a clean working tree afterward means nothing
drifted. The Rust loader is `rust/phon-conformance/tests/corpus.rs`; Swift and
TypeScript loaders read the same files once those implementations exist.

## Layout

```
conformance/cases/
  <case>/
    <label>.phon     self-describing bytes of one schema (id baked in)
```

A case is a batch of mutually-referential schemas resolved together, so
recursion (e.g. `linked_list`: `Node` ⇄ `Option<Node>`) gets correct ids. The
current cases cover a plain struct, an enum with all four payload shapes, the
recursive linked list, generics (`Pair<A,B>` + `Holder<T>`), every container
kind, and the special kinds (dynamic, external with and without metadata,
channel).

## Scope

This is the **schema-level** oracle: schema bytes and schema identity. When the
compact codec and the `Value` (self-describing) codec land, cases will gain
sample-value files (a value in each wire mode) and the loaders will check those
round-trip too. The directory format extends without breaking what's here.

## Why bytes, not a portable phon file

The corpus can't be stored *as* compact phon, because the codec is the thing
under test — that would be circular. So cases are raw self-describing byte files,
each self-contained (it carries its own id).
