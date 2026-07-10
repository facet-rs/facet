+++
title = "Store"
weight = 3
+++

The content-addressed value store: handles, interning, molten working
memory, freeze/publish, snapshots, and reload semantics.

> r[machine.store.handle-opaque]
>
> [SETTLED] Exactly one handle type exists: a store-owned newtype (`Handle`,
> u32 backing — the arena does not justify 64 bits). Callers never derive
> handles from positions, never interconvert with raw integers, and never
> learn a handle's meaning from iteration order.

> r[machine.store.handle-store-assigned]
>
> [SETTLED] Handles are assigned by the store at allocation and stored with
> the entry. Deriving a handle from `enumerate()` position at snapshot time is
> banned (any filter/compact/reorder silently re-means every handle).

> r[machine.store.handle-doc-lifecycle]
>
> [SETTLED] Every handle type's documentation states owner, lifetime, and
> reclamation strategy. "Append forever, deliberately" is an acceptable
> strategy; an unstated one is not.

> r[machine.store.reclamation-policy]
>
> [SETTLED] Handles and identity metadata are stable for the lifetime of a
> machine snapshot; resident bodies are not append-only. A handle slot is
> `Resident(bytes)` or `Evicted { schema, content_hash, sources }`. Policy may
> evict and rehydrate bodies without changing handles, identities, or claims.
> Value DAG reachability, active leases, roots, hotness, recomputation cost, and
> source availability govern reclamation. Memo candidates and claim metadata
> are independently evictable. Reusing a numeric handle for a different value
> within one snapshot is forbidden.

> r[machine.store.dedup]
>
> [DESIGN] The store indexes every value by `(SchemaRef, HandleTier,
> ContentHash)` and returns the existing handle for identical content,
> observable via a `StoreAlloc { deduped }` event. Tier is a dedup-key axis
> (pending and realized slots are distinct) even though tier never enters hash
> bytes (`machine.identity.tier-not-in-hash`); a pending slot's `ContentHash`
> component is its promise identity, a realized slot's is its value identity,
> and they are NOT equal (`machine.identity.pending-identity`), so the two
> never contend for one slot. Doc-90's two-element key is a documented error.
> Note: only realized-tier values persist (`machine.persistence...`), so tier
> is constant at the persistence boundary and the persistence key needs no tier
> axis.

> r[machine.store.immutable-bytes]
>
> [SETTLED] Store bytes are immutable and content-addressed. Value memory
> REFERENCES them — slices into the append-only arena (stable by
> construction) or Arc — and never copies. A whole-store copy is a bug
> regardless of what gates it; elision of a banned copy is not a fix.

> r[machine.store.molten-vocabulary]
>
> [SETTLED] "Molten" means mutable, in-flight, not interned. Molten arenas die
> at freeze or publish-once. The molten→interned transition is named
> `freeze`/`publish`; a name like `intern_molten_word` violates Law 8 and this
> rule.

> r[machine.store.publish-once]
>
> [DESIGN] Loop-shaped computations keep their working state molten and
> private for the loop's whole extent, publishing once at the fixpoint — one
> identity transition, one demand completion. Identity work inside a
> convergence loop is a design bug (the SOA/fixpoint lesson: per-mutation
> laundering re-interned the entire state).

> r[machine.store.snapshot-no-clone]
>
> [SETTLED] Store snapshots are views: iterators of borrowed entries or Arc'd
> bytes. A snapshot that clones the world is a perf event masquerading as a
> read.

> r[machine.store.reload-is-program-swap]
>
> [DESIGN] A code reload swaps program/fns/descriptors and clears only the
> transient trace. The store and memo survive; a warm re-demand after reload
> spawns nothing when the function's closure hash is unchanged.
> Reload-warmth's granularity is exactly "did this function's closure hash
> change." (Preserved from `Driver::reload`.)

> r[machine.store.value-bundle-portability]
>
> [DESIGN] Values are portable across machine instances via a serialized
> bundle (schema, tier, bytes, content hash, taint — plus code). Bundles
> carry values only: no memo entries, no read-sets. Value portability and
> warm-memo persistence are two different features with two different trust
> stories; conflating them is the documented doc-90 error. (Preserved from
> `export_value_bundle`/`import_value_bundle`.)

> r[machine.store.construction-services]
>
> [DESIGN] Store and molten construction is part of the scheduler-service
> surface (census class C): alloc, read, tag, record-update, freeze,
> molten-dup, collect. Each is witness-typed where it reads
> (`machine.receipt.witness-reads`) and none is reachable through a
> non-recording path.
