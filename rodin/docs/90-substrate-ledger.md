# 90 — Substrate ledger: what the machine provides, and what it doesn't

rodin-core ran on plain Rust: no memoization, no incremental recomputation, no
content-addressing, no read-tracking — so it built all of that by hand, and that
machinery is a large fraction of its ~10K lines. The vix machine provides most of
it natively, and expressing the resolver as demand over the machine yields those
for free.

But "the substrate provides it" is a claim *about the substrate*, and this ledger
only makes it where the source backs it. Where the machine does **not** provide
something rodin-core built — or provides it differently than a first guess
assumes — that is called out as **Rodin (or the store) must build this**, so the
reflex "that's the substrate's job" is never applied blind.

Every line below cites `vix/src/machine/driver.rs` (unless noted) by **symbol**,
not line number, so the citations survive refactors.

## The mapping — verified against the source

### Interner → content-addressed store ✓ (one caveat)

**Provides.** `ValueStore` indexes every value by
`by_content: HashMap<(schema, ContentHash), i64>`; `ValueStore::alloc` computes
`content_hash` and returns the existing handle for identical content
(`DriveEvent::StoreAlloc { deduped }` observes the dedup). Value identity *is* the
content hash; equal values are already the same handle. No per-value interner, no
`PkgIx`/`SourceIx`/`FeatIx` index types.

**Caveat (the old "no interning step, no index types" was too strong).** Schema
*strings* are still interned to small integers (`Driver::intern_schema_ref`), and
values pass through a mutable working layer (`FleshStore`, `FleshValue::Interned`)
before they freeze into the content-addressed store. So: *values* are
content-addressed (no interner); *schemas* are interned. rodin-core's identity
interner is subsumed; its schema-ref equivalent is not absent — it is
`intern_schema_ref`.

### Read-sets → field-granular projection tracking ✓ (tracked yes; exposed to resolver code, no)

**Provides.** `ProjectionReadSet` records
`ProjectionRead { arg_index, path, observed: ContentHash }`, and `ProjectionPath`
distinguishes `Field { schema, field_index }`, `Tag`, `MapGet { key_hash }`,
`Whole`, `TreePath`, … . Reads are tracked at **field / tag / map-key
granularity**, not whole-value. A memoized function that reads only `.major`
records exactly that field, with its observed content hash. On re-demand with
changed inputs, `DriveEvent::MemoProjectionHit { verified }` reuses the warm
result iff every recorded projection still matches — the machine re-runs only when
a field the function *actually read* changed. You do not track reads; the machine
does, finely.

**Rodin / machine must build this: read-set *exposure*.** The read-set lives in
`MemoEntry.read_set`, private to the driver and used only for memo verification.
It is **not** surfaced to vix / resolver code. Doc 50's *read-set widening* ("the
derivation only read these fields; generalize the no-good to every version
agreeing on them") needs the resolver to *observe its own read-set* — a capability
the machine does not currently expose. Field-granularity makes read-set widening
*sound in principle*; but until the machine surfaces read-sets to resolver code,
doc 50's read-set-widening must be driven inside the machine or fall back to
declared-structure widening. **This is the open Phase-2 question — a gap, not a
given.**

### Warm facts → in-process memo + reload, WITH verification, NO cross-process persistence ✗ (old claim wrong twice)

**Provides.** The memo survives a code `reload` — `Driver::reload` swaps
`program`/`fns`/`descriptors` and clears only `self.trace`, leaving `memo` and
`store` intact — so a warm re-demand after a reload costs no task
(`warm_demand_spawns_nothing`: seed memo → demand → `MemoHit`, zero `Spawned`).
Reuse is **verified**, not assumed: `MemoProjectionHit` / `MemoSemanticHit
{ verified }` re-check the recorded read-set / declared comparators against the
new arguments before serving the warm value.

**Correction 1 — there IS a verify step.** The previous ledger said warm facts
"cannot be stale because identity is content… no serialize/verify step." False:
`MemoProjectionHit` *is* a verify step. rodin-core's `WarmFactVerifier` is not
eliminated by content-addressing — it is *performed automatically* by the
projection memo.

**Correction 2 — "carried across runs" is NOT provided.** `memo` and `store` are
in-memory (`HashMap` / `RefCell<ValueStore>` fields on `Driver`); there is **no**
serialization path in the machine (a crate-wide grep for
`serialize`/`to_disk`/`from_disk`/`persist` on the store or memo returns nothing —
the only serializers are IDE bindings and AST encoding). "Warm reload" means
same-process reload of code against a live store — *not* rodin-core's
serialize-a-no-good-bundle-and-reload-it-in-a-fresh-process. **Cross-process warm
facts would need a store-persistence layer that does not exist.** (Consistent with
the open/proprietary split: persistence-as-a-service is a store feature, not
on-device substrate.)

### Proof graph → the read-set is the reuse certificate; NOT a replayable/explainable derivation ✗ (old claim overstated)

**Provides.** The reusable soundness certificate for a memoized value is its
`read_set` (`MemoEntry.read_set`: what was observed, with content hashes),
re-verified on reuse. That is what makes warm reuse sound — and it is enough *for
reuse*.

**Correction.** "The demand graph *is* the derivation… the machine's evaluation is
the proof" overstates. The trace (`DriveEvent`s in `self.trace`) is transient —
`reload` clears it — and there is no persisted `ProofNode` / `ProofRule` DAG. The
machine retains a *verification certificate* (the read-set), not a *walkable
derivation*. For soundness of reuse that suffices. For **explanation** —
rodin-core's `ProofGraph` purpose, "*why* was this selected / why did this no-good
fire" — the machine keeps nothing you can walk after the fact. **If Rodin wants
human-facing derivations, it builds them itself; the substrate does not provide
them.**

### Counterfactuals → incremental recompute ✓; the diff is resolver-level

**Provides.** Incremental invalidation is real: edit an input, re-demand, and only
the blast radius recomputes — where read-sets still verify, `MemoProjectionHit`
serves warm; where they don't, the machine `Spawned`s a recompute. That is exactly
rodin-core's "which selections change under this edit," driven by the projection
memo.

**Caveat.** "The diff is the difference of two demanded results" — the machine
gives incremental *recompute*, not a `SelectionDiff` primitive. Computing *which*
selections changed is resolver-level: demand both results, diff them. Cheap, but
not a machine feature.

## The rule this encodes (sharpened)

Before interning a value, hand-rolling a canonical form, or tracking what was read
— stop; the store content-addresses values, and the projection memo tracks reads
finely and re-verifies warm reuse. But three things the reflex must **not** assume
the substrate hands over:

1. **Read-sets exposed to resolver code** — they are internal; read-set widening
   (doc 50) needs an exposure decision.
2. **Cross-process persistence of learned facts** — no serialization exists; that
   is a store feature.
3. **A walkable proof/derivation for explanation** — only a verification
   certificate (the read-set) is retained.

Everything else in rodin-core's scaffolding — interner, read-set bookkeeping,
warm-fact *verification*, incremental invalidation — the machine genuinely
subsumes, and the citations above are where.
