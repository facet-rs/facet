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
>
> A compact discriminated frame is one structural value, not independent words:
> its selector determines the machine shapes permitted in the shared payload
> region. A payload word may therefore be `Scalar` in one variant and a typed
> handle in another without becoming an untyped scalar-or-handle escape hatch.

> r[machine.value.access-strategy]
>
> [SETTLED] Value construction and reads go through the descriptor's Access
> strategy. Inline offset arithmetic, field ordinals, and hardcoded widths at
> call sites are forbidden — a layout dependency the compiler cannot check is
> hand-rolled ABI. Descriptors and stores are not all-`pub` field bags; the
> invariant-preserving path is the only path.
>
> The same rule applies inside Weavy. A verified discriminated constructor
> establishes the selector-correlated payload shape and canonical inactive
> bytes. Verified variant dispatch first validates that the selector is one of
> the descriptor's declared variants, so an invalid selector faults instead of
> selecting a catch-all or final arm. A verified discriminated projection checks
> both selector validity and the requested variant before it extracts a field.
> Ordinary word copies may preserve a complete structural shape, but may not
> narrow a selector-dependent union to one of its leaves.

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
> [SETTLED] There is no shadow taint bitmap or graph-side taint attached to an
> otherwise ordinary value. A secret is an explicit `Sealed<T, Policy>` leaf
> with ciphertext-derived identity and capability-gated reveal. Aggregates gain
> per-leaf precision through their ordinary structure: copying or projecting a
> sealed leaf preserves the wrapper; unrelated siblings remain ordinary.
> Primitives whose derivation is opaque may return a whole-output sealed wrapper.
> Provenance and reveal facts live in receipts/journals rather than hidden value
> metadata.

> r[machine.value.structural-order]
>
> [DESIGN, round 10] `<=>` is the structural comparison: total, equality-consistent,
> derived, not overridable (`machine.identity.never-consults-order`). Every claim
> that "every value is ordered" rests on these base cases, which are hereby the
> definition, not an implementation detail:
>
> - **Int**: numeric.
> - **Bool**: `false < true`. **Unit**: one value.
> - **Float**: IEEE `totalOrder`, NaN canonicalized to one bit pattern, so `<=>` is
>   reflexive and equality-consistent. (Precedent: `TotalF64`, `vix/src/machine/value.rs:16`.)
> - **String, Path**: by Unicode scalar value, locale-free.
> - **Blob**: byte-lexicographic.
> - **Struct**: field-wise, in DECLARATION order. Nominal types compare their name
>   first (it is in the canonical encoding); a nominal and a structural type of the
>   same shape are different types and never compare.
> - **Enum**: by variant DECLARATION position, then payload. This is the mechanism a
>   type uses to carry an ordering rule its fields cannot (`PreTag`, semver).
> - **Array `[T]`**: lexicographic by index, shorter-is-lesser on a prefix.
> - **Map<K,V>**: lexicographic over rows in key order. **Set<T>**: as `Map<T,()>`.
> - **Option/Result**: as enums, by variant position.
> - **Function**: by the stable identity of its definition (the canonical AST of
>   everything it transitively references — `machine.identity` closure identity).
>   **Closure**: definition identity, then its capture record structurally.
> - **Tuple**: as a struct with fields `0, 1, …`.
>
> **`Stream<K,V>` and `ByteStream` are codata and have no structural order.**
> They have recipe identity while live and completed semantic content
> (`Map<K,V>` or `Blob`) once drained. They may be record fields and inter-island
> edges, but are not map keys or sortable while live
> (`machine.identity.streams-cross-island-edges`).
>
> **Values are DAGs.** A runtime value cannot contain itself, so structural
> comparison terminates (`machine.store.*`: values form a DAG by construction).
>
> **Cost obligation.** `<=>` MUST short-circuit on identity: two values whose
> identities are equal are `Equal`, without a walk. Identities are carried in a slot
> (`machine.identity.hash-at-construction`), so this is a load, not a hash. Without
> it, `by_key(|x| x.big_tree)` is total and unusable. Deep comparison of two
> *distinct* large aggregates remains the caller's cost, and is the reason `by_key`
> extracts a key rather than sorting by the whole value.
