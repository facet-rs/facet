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
conformance/
  cases/
    <case>/
      <label>.phon   self-describing bytes of one schema (id baked in)
  values/
    <name>.phon      self-describing bytes of one Value
```

A *schema case* is a batch of mutually-referential schemas resolved together, so
recursion (e.g. `linked_list`: `Node` ⇄ `Option<Node>`) gets correct ids. The
cases cover a plain struct, an enum with all four payload shapes, the recursive
linked list, generics (`Pair<A,B>` + `Holder<T>`), every container kind, and the
special kinds (dynamic, external with and without metadata, channel).

A *value case* is one self-describing `Value`. The values cover every case the
codec emits — null, bool, the integer/float widths, string, bytes, char, array,
object — and the extended kinds: uuid, qname (namespaced and local), and every
datetime shape. The oracle for a value is: decode it, re-encode, and get
byte-identical output (its `Value` codec matches Rust's), since values carry no
schema and no id.

## Scope

This covers schema bytes + identity and self-describing `Value` bytes. When the
compact codec lands, value cases will gain a compact rendering against a schema
and the loaders will check that round-trips too. The directory format extends
without breaking what's here.

## Why bytes, not a portable phon file

The corpus can't be stored *as* compact phon, because the codec is the thing
under test — that would be circular. So cases are raw self-describing byte files,
each self-contained (it carries its own id).
