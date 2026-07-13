# 10 — Package identity

Ground truth: cargo. This doc fixes what makes two package nodes the *same*
resolution domain — the atom everything else quantifies over.

## Identity is a triple, not a name

A package's identity is `(source, name, compat-class)`:

- **source** — provenance. crates.io, an alternate registry, a git dependency
  (url + rev), or a path dependency. The same `name` from two provenances is two
  identities; cargo resolves and locks them independently.
- **name** — the crate name.
- **compat-class** — the semver coexistence bucket (below). This is why `serde
  1.x` and `serde 2.x` can both appear in one lockfile: they are different
  identities, so they don't compete for a single version slot.

Two nodes with equal triples are the same domain and must resolve to one
version; unequal triples resolve independently.

## The compat-class rule (cargo's coexistence semantics)

cargo lets two versions of a crate coexist iff they are semver-*incompatible*.
The compat-class is the equivalence key for "semver-compatible with," keyed on
the position of the first non-zero version component:

- major ≥ 1 → class is `major` (1.4.2 and 1.9.0 share class `1`; 2.0.0 is class
  `2`).
- major = 0, minor ≠ 0 → class is `0.minor` (0.4.x and 0.5.x are distinct
  classes — the 0.y footgun).
- major = 0, minor = 0 → class is `0.0.patch` (each 0.0.z is its own class).

Same class ⇒ the two versions compete for one slot. Different class ⇒ they
coexist.

## Compat is optional in identity

An identity may carry *no* compat-class. Unclassed identity names the package
abstractly — before any requirement has narrowed it to a coexistence domain
("is this crate in the graph at all," independent of version line). Classed
identity names one coexistence domain within it. (Confirm the exact role of the
unclassed form while distilling the search — it appears as the pre-narrowing
identity.)

## What is NOT part of this

Interning and index assignment: rodin-core gave identities `PkgIx`/`SourceIx`
integers so they were cheap to compare and store. The machine content-addresses
every value; equal identities are already the same handle. There is no interning
step. See `90-substrate-ledger.md`.
