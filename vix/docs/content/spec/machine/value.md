+++
title = "Values"
weight = 8
+++

The value model: how values are typed, constructed, read, and discriminated.

> r[machine.value.word-newtype]
>
> [SETTLED] The substrate-boundary machine word is `Word(i64)`, a newtype.
> Bare `i64`/`u64`/`usize` for domain values is forbidden.

> r[machine.value.domain-newtypes]
>
> [SETTLED] Domain quantities above the substrate — value handles, fn refs,
> slot indices, lengths, ids — are distinct newtypes. Two different meanings
> never share an integer type.

> r[machine.value.tag-dispatch]
>
> [SETTLED] Value discrimination is by descriptor/tag. Comparing an enum's
> name or variant's name as a string is banned. (The old driver compared
> `"Option"`/`"None"` strings on a hot path.)

> r[machine.value.access-strategy]
>
> [SETTLED] Value construction and reads go through the descriptor's Access
> strategy. Inline offset arithmetic, field ordinals, and hardcoded widths at
> call sites are forbidden — a layout dependency the compiler cannot check is
> hand-rolled ABI. Descriptors and stores are not all-`pub` field bags; the
> invariant-preserving path is the only path.

> r[machine.value.proof-carrying-force]
>
> [SETTLED] A function that establishes an invariant returns a type carrying
> it: forcing a tree yields `ConcreteTreeRef`, not the same raw handle. An
> "impossible by construction" match arm holding a string error means the
> construction was never encoded in types — fix the type, delete the arm.

> r[machine.value.lazy-reads]
>
> [SETTLED] Read paths never force eagerly. Pulling one entry from a
> merge/exec tree must not materialize the whole tree; entries are pulled as
> typed refs on demand.

> r[machine.value.content-refs-never-stringify]
>
> [DESIGN] Content references (tree entries, hashes, handles) are never
> stringified into `String→String` maps. Identity round-tripping through
> strings is banned.

> r[machine.value.option-no-store-alloc]
>
> [DESIGN] Constructing `Some`/`None` does not intern a store value. Options
> are tag-discriminated words; `None` is a per-schema singleton with
> const-known identity. (The old machine content-addressed every option
> construction; `map.get` minted store values.)

> r[machine.value.typed-pull-api]
>
> [DESIGN] Value inspection is a typed pull API — schema+layout probes over
> store memory, facet-style views. The push-rendering mirror
> (`RenderedValue`/`render_*`) is removed in order: probes land, rim
> assertions migrate, renderer dies. Killing it earlier orphans the test
> suite's observation surface; killing it later re-entrenches it.

> r[machine.value.taint-provenance]
>
> [OPEN] Sub-value taint granularity is unresolved between two prior rulings:
> `vix-spec.md` V30 ("provenance is graph-side; sub-value taint is a
> non-goal") and `secrets-as-sealed-values.md` Q1 (per-leaf structural
> precision where the machine understands the derivation, whole-output taint
> only for opaque exec). These are mutually exclusive as written. Until
> adjudicated, the rewrite implements taint-as-identity (see
> `machine.identity.taint-in-identity`) at whole-value granularity and blocks
> any per-leaf work on this rule.
