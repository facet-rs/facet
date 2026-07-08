# HASH-AS-FIELD / memory-identity (the second epoch)

Status: **design, committee-reviewed → REVISE-then-proceed folded in.** Both seats
returned (seat 1 / opus: PROCEED with six binding rulings; seat 2 / codex:
REVISE-then-proceed with five P1s). This revision incorporates both, per Amos's
adjudication where they conflict. Implementation is charter-gated, not authorized by
this doc. Docs-only lane; coordinates with the standing perf lane landing tactical
levers in the same `driver.rs` territory — this document touches no code.

### Grounding provenance (codex asked for reproducibility — here it is)

The required grounding is **partly out-of-tree by design** and reviewers should know
where each piece lives:
- `RESURRECTION.md` — **gitignored** in facet-cc (`.gitignore` excludes `/RESURRECTION.md`
  and `/vix/docs/`), archived on the private `vix-docs-archive` branch. Real, not
  branch-local.
- `capabilities-ambient-vs-materialized.md` — lives in the **vixen** corpus
  (`docs/design/`), a different repo. The zero-padding law and second-epoch
  authorization are dictations there.
- `vix/docs/content/redesign/2-hashing-flesh.md` — present in the facet-cc working
  tree but under the gitignored `/vix/docs/` prefix; it predates the sha2→blake3 swap
  and the flesh→molten rename, so read it as *thesis*, not current code.
- Flame evidence (the 23.4 / 29.1 / 13.6 / ~66% / ~95,000× figures) is the **perf-lane
  solve-class run**; the perf lane is committing the flame notes. **Cite the committed
  note paths here once they land** — until then these figures are attributed to that
  run and are not independently reproducible from this branch. Do not bank them as
  reproduced.

All `driver.rs:NNNN` code anchors below are verified against the live file.

---

## 0. The existential framing

A native reference solve runs the *identical* index at **~95,000×** vix on solve-class
workloads (perf-lane flame). Of vix's solve time, the flame attributes **~66%** to
*recomputing identity bytes the machine already knew* — three **addressable trunks**
(the word matters; see §6 — "addressable" is not yet "removed"):

| trunk | flame share | live site |
|---|---:|---|
| blake3 at every allocation | **23.4%** | `ValueStore::alloc` recomputes a full-value digest on every intern — `driver.rs:1407`, hashing at `:1419` |
| map re-canonicalization per intern | **29.1%** | `alloc_map` → `canonical_map_pairs` re-canonicalizes, re-hashes every key+value, sorts, dedupes from scratch — `driver.rs:1466`, `:10376` |
| observation re-hashing | **13.6%** | `projection_observation_hash` re-derives a digest per demanded read — `driver.rs:9980` |

This is repeated work over *immutable* data. Interned store entries are immutable, so
their identity is fixed the instant they exist. The proposal: stop recomputing it,
**store it as a field** — computed once at the write, carried incrementally while
molten, read as a field load forever after.

Amdahl caveat, stated up front (opus ruling 6): removing 66% of flame is **≈ 2.9× at
best** (`1 / (1 − 0.66)`). This proposal is the *existential precondition* — it removes
the trunk that makes 50× unreachable — but the JIT lane and memo-granularity legs (§6)
are **load-bearing, not garnish**. And the 29.1% map trunk is **contingent** on the map
construction chosen (Q1) — not banked.

---

## 1. The identity slot

**Claim.** Content identity is a *property of a value*, not a computation over it. Every
non-inline value class gets a descriptor-reserved slot holding its identity. The slot's
*representation* differs by lifecycle stage; its *meaning* (the blake3 content hash of
the canonical value, taint-composed) does not.

### Per value class

- **Interned store entries** — already carry `content_hash: ContentHash` in `StoreEntry`
  (`ContentHash = [u8;32]`). Today it is *recomputed* at `alloc` (`:1419`) then cached;
  under this proposal the cached field *is* the slot and the recompute path is deleted —
  the digest arrives already-finished from the molten carried state (§2) or the flat
  stencil (§4). No layout change; we change *who writes it*.

- **Molten (carried) values** — `MoltenEntry` holds `carried_array_hash:
  Option<CarriedArrayHasher>` (`:1141`), wrapping a live `blake3::Hasher` (`:1068`) — the
  slot *for arrays only*, in incremental representation. The proposal generalizes it to
  every molten class (`MoltenValue::{Record, Map, ArrayWords}`, `:1145`) — but the
  molten slot is a **validity-tracked cache of a future identity, not a write-once
  field** (see §2; codex P1).

- **Inline scalars — exempt.** `schema_is_inline_word` values (`:2037`) are their own
  identity: the word bytes are the canonical encoding, memcmp-comparable. `hash_value_into`'s
  `Access::Scalar` arm already hashes raw word bytes directly. No slot.

### The slot carries base identity AND taint (codex P1 #3)

The array path today finalizes the base array hash, then applies `hash_with_taint` over
collected child taints (`finish_array_element_hash` at `:1852`, `hash_with_taint` wrapping
it; taint combine at `:1846`). A generalized slot **must state that it carries base
identity plus a composed taint**, and taint invalidation must not be silently dropped:
- the carried state holds the incremental *base* digest;
- taint is composed from children and applied at finalize (as today);
- any mutation that changes a child's taint invalidates the carried taint composition,
  exactly as a changed child identity invalidates the base (§2).

### Representation

| stage | slot | meaning | write-once? |
|---|---|---|---|
| molten, mutable | `Option<CarriedIdentity>` (base midstate + taint accumulator) | in-progress digest of a *future* identity | **no — validity-tracked, droppable** |
| interned, frozen | `ContentHash` (`[u8;32]`) | the finalized, taint-composed digest — the value's permanent name | **yes — never invalidated** |

HandleTier stays out of the slot (invariant preserved from the first epoch): it keys
`ValueStore::by_content` (`:972`, `:1380`) but never enters the hash input.

---

## 2. Lifecycle: two layers, not one "write-once" (both seats, layered per Amos ruling 2)

The proposal previously said identity is "invalidated never." That is true for the
**interned** slot and false for the **molten** carried state. The corrected model is two
layers:

### Layer A — interned `ContentHash`: write-once, read-forever

Computed once at intern (`finish_hash`, `:740`), immutable thereafter, never
invalidated. Read as a field load. This is the absolute, and it is genuinely write-once.

### Layer B — molten `CarriedIdentity`: a validity-tracked droppable cache (codex P1)

Molten carried state is *not* a finished identity; it is a cache of the identity the
value **will** have when frozen. It is valid only while every folded child identity
equals the **final post-intern identity** that will appear in the frozen bytes. Rules:

1. **Computed at intern** — freezing finalizes Layer A once: O(1) `finish_hash` of the
   carried midstate if valid, else a flat-bytes stencil pass (§4) over the canonical
   bytes.
2. **Carried incrementally on molten mutation** — fold the changed child's
   post-canonical identity into the midstate (arrays: append; maps/records: the ordered
   structure of Q1/Q2). Also fold taint (§1).
3. **Dropped-and-recomputed when it cannot fold a *final* child identity** — this is the
   rule, not an exception. Any mutation whose child is not yet at its final canonical
   identity must either force that child to its store identity before folding, or mark
   the carried state dirty and recompute at intern.

### The motivating bug, named (opus unifying finding + array carry defect)

The live `changed_words → carried_hash = None` (`:2135`) is **not incidental** — it is
Layer B's validity rule already firing. Root cause: `update_array_element_hash` folds
the child's `ContentHash`, but that hash is taken on the **pre-intern** child
(`canonical_word_hash_in_store(*word)` before interning); if the child *re-canonicalizes
at intern*, its final identity differs, the midstate is stale, and the carry is dropped.

Consequence, and it is the spine of both reviews:
- **Array of inline scalars (the trail loop):** scalar canonical hash is stable across
  intern → `changed_words` stays false → carry survives → the measured 50× lever. **This
  is the only case the tree proves.**
- **Aggregate-of-aggregates (what the 29.1% map trunk *is* — maps of package→version
  records):** children re-canonicalize at intern → `changed_words` fires → carry dropped
  → O(n) recompute. **The generalization does not materialize for the biggest trunk by
  assertion.**

### The engineering that makes drops rare (opus ruling 1)

Make **molten-canonical == intern-canonical** so a child identity is stable the instant
it is folded, and Layer B drops become the exception rather than the rule on nested data:
- **records/scalars:** the zero-padding + flat-memory law (§3) makes molten bytes already
  canonical → intern does not re-canonicalize → the folded child identity is final;
- **maps:** intern's re-canonicalization *is the sort* (`canonical_map_pairs` sorts by
  key hash, `:10404`); an **ordered incremental structure** (Q1 construction b) folds in
  canonical key order, so molten-fold == intern-fold and the drop dissolves for maps.

**Fixture (both seats):** an array/map/record **of aggregates** asserting `changed_words`
(and its map/record analogues) never fires post-flat-memory. Until that fixture is green,
the map/record win is unproven — not banked (§6).

This is the strongest argument *for* the second epoch: flat-memory is not merely a perf
lever, it is the **soundness enabler** for the carried-hasher generalization. Gate 3
(carried maps/records) therefore *depends on* gate 1 (flat memory) + the Q1 structure —
a dependency, not just an ordering (§5).

---

## 3. Zero-padding law integration (Amos-directed; obligations completed by both seats)

`capabilities-ambient-vs-materialized.md` settles it: **weavy-declared layouts mandate
padding == 0, no exceptions** (fresh pages are zero; hash-at-intern already touches every
byte; memcpy preserves canonicity; the one recurring tax is variant-switch slack). This
buys **flat-bytes blake3** (identity = blake3 over raw memory, no descriptor-walk —
replacing the unconditional `hash_value_into` walk at `:11211`) and **memcmp equality**.

### Write-side obligations (the completed list)

Fresh construction is already zeroed: `STORE_ALLOC` starts `vec![0u8; layout.size]`
before writing tag+fields (`:3200`); `alloc_doc_variant` likewise (`:7707`). The gaps are
in **mutation**:

- **frame slots zero on entry** — weavy `Init::Zero { offset, size }` (`weavy/src/ir.rs`);
- **narrowing-field-overwrite re-zero (opus ruling 3).** A molten write of a value
  narrower than its slot (stage-4 writes `to_le_bytes()[..field.size]`) leaves stale high
  bytes if the slot previously held a wider value → mis-identity. Every narrowing field
  overwrite must re-zero the slack. Same defect class as variant-switch, broader trigger;
- **inactive-enum-payload zeroing — the *whole* payload region, not just declared padding
  (codex P1 #4).** On a variant switch from a larger to a smaller payload, stale payload
  bytes remain unless the switch zeros every byte of the enum payload region **not owned
  by the new active variant**. These stale bytes are *not* `RecordByteOwnership::Padding`
  for the selected variant — so the canary must treat inactive-variant bytes as part of
  enum canonicality, not only declared padding.

### Atomic variant switch (both seats — one primitive)

Make variant switch a single primitive, never two folds:
1. determine old and new active variant;
2. **zero** every payload byte not owned by the new variant (incl. old-payload slack);
3. write the **tag**;
4. write **and fold** the new payload region exactly once.
(zero → tag → write → fold, atomic — no double-count, no race.)

### The padding canary runs in CI *always*, not only under `debug_assertions` (opus ruling 3)

Load-bearing distinction: the force-copy and cross-lane differentials (§5) are
**relative** — they catch *divergence between two paths*. A **shared** zero-init bug is
wrong *identically* on both paths, passes every differential, and produces globally wrong
identity → false cache hits → **wrong builds** (the existential failure). The canary — a
cheap zero-check over declared padding *and inactive-variant payload* ranges at intern —
is the **only absolute guard**. Run it in every CI corpus run. (Gotcha: the canary
predicate must be computed separately and asserted, never placed *inside* a
`debug_assert!` side-effect position — release compiles those out.)

### The facet-bridge canonicalization boundary

The law governs weavy-declared layouts only. facet-*discovered* values (rustc dictates
layout; padding is whatever the Rust ABI left) canonicalize **at the bridge**: the copy-in
re-layouts into weavy-declared, zeroed form and *mints* the guarantee there. The identity
slot of a bridged value is computed on the canonical copy, never on raw Rust bytes — the
first-epoch ratified boundary (post-V10, additive, hash-neutral) unchanged.

---

## 4. The blake3 stencil

**Answered from copypatch's actual capabilities.** copypatch "runs and patches bytes — it
never encodes an instruction or interprets" (`copypatch/src/lib.rs`): a stencil's
arithmetic is a **host-compiled native body**, patched with immediates. weavy's IR
(`weavy/src/ir.rs`) is a *memory/aggregate/control* IR (`MemoryOp`, `InitOp`,
`AggregateOp`, `Control`) — it has **no arithmetic/bitwise op vocabulary** (the `Add` in
`weavy/src/async.rs` is a toy demo op).

This resolves "needed IR ops (rotate at minimum)" **by reframing**:

> **No new arithmetic IR ops are required.** blake3's compression (add/xor/rotate
> G-mixing) is **not** lowered to vix/weavy IR. It stays in a **stencil body** compiled
> from the `blake3` crate by the host toolchain — `-O by construction`, exactly
> copypatch's value. The only IR surface is an `Intrinsic` node (`weavy/src/ir.rs:39`
> `Intrinsic(Intrinsic)`) that the JIT patches with input pointer/offset, output-digest
> pointer, and length immediate. Rotate/xor/modular-add never appear as vix ops.

**IR orchestration.** The JIT orchestrates: (a) obtain flat canonical bytes (a
`ScalarRun`/`Move` region under the zero-padding law), (b) invoke the blake3 intrinsic
stencil over that region, (c) store the finalized digest to the reserved slot offset (a
`ScalarCopy`). Only (b) is new, and it is an intrinsic, not an op-set expansion.

**Interp-lane callability — load-bearing for correctness.** The interpreter calls the
identical native `blake3` routine as an ordinary Rust call; the JIT patches the same
routine as a stencil. One compiled compression body ⇒ **byte-identical identity across
interp and JIT by construction** — no "the JIT hashes differently" failure mode. Same
guarantee `finish_hash`/`blake3::Hasher` gives today (`:740`), carried into the stencil
world.

**Stencil-state ABI is process-local only (codex non-blocking note).** `blake3::Hasher`
is a Rust type, **not** a stable, portable, cross-version memory contract. The carried
midstate (Layer B) is therefore valid **only within a single process** — it is molten
scratch, never persisted, never sent cross-version. Persisted/cross-process identity is
always the *finalized* `ContentHash` (Layer A), never a carried midstate. The stencil ABI
spec must say this explicitly: the midstate-update stencil operates on
process-local `blake3::Hasher` state; only finalized 32-byte digests cross any boundary.

**Stencil inventory.** One core compression stencil (G-mix + finalize) covers the
flat-bytes path; one midstate-update stencil (fold one 64-byte block into carried state)
covers Layer B. Both live beside `weavy/stencils/{async_ops,hostcall,task_ops}.rs`.

---

## 5. The second epoch

RESURRECTION IDENTITY AMENDMENT (2026-07-08): *"the epoch's encoding-hashes are NOT
sacred… identity migrates to canonical-memory hashing as a SECOND sanctioned epoch — its
own break, own gates, committee-ratified."* This is the migration plan.

**What changes.** First epoch (V3/V1/V2, on `rodin`) hashes canonical payload
*encodings* — `hash_value_into` walks the descriptor and hashes a domain-separated,
field-by-field encoding. Second epoch hashes canonical *memory* — blake3 over the zeroed
flat bytes of the weavy-declared layout. Same algorithm (blake3), different input. Every
content hash changes value.

**What breaks.** Everything keyed by content hash: `by_content` dedup keys, memo keys,
`.vix-cas` store keys, `ReadObservation`/projection hashes. Consistent within a process;
observable only across the epoch boundary.

**Bridges — none needed.** No cross-process persistence exists yet; the only persistent
dependent is `.vix-cas`, regenerable by construction (a memo cache, not source of truth).
Clean cutover — no compat shim, no dual-hashing window. A prior-epoch `.vix-cas` is
treated as cold (miss-and-recompute), never migrated.

**Oracles.**
- **Force-copy differential** (`VIX_FORCE_MOLTEN_COPY`): every corpus fixture with molten
  reuse forced off must produce **byte-identical** content hashes on the reuse and
  force-copy paths (memcmp-exact under §3). Catches canonicalization/zero-padding bugs *at
  the value*. Note: **relative** — see the canary (§3).
- **Interp/JIT cross-lane differential** (§4): the shared stencil must agree in both lanes
  corpus-wide. Also **relative**.
- **First-vs-second-epoch structural check** — assert the epoch swap changed *names*, not
  *equivalence classes*: equality preservation **and INJECTIVITY** (opus ruling 5;
  distinct-under-epoch-1 → distinct-under-epoch-2). A false cache hit is a padding/combine
  **collision** (two distinct values → one hash); only injectivity over the corpus catches
  it. Run to corpus exhaustion — the **absolute** identity guard the relative differentials
  cannot be.
- **rodin-vs-cargo build differential** — survives the epoch; the correctness backstop
  against a wrong-identity build.

### Gate sequence — SPLIT so array/whole-value wins never wait on map research (codex P2, opus ruling 5)

1. Define `StoredIdentity` (Layer A) / `CarriedIdentity` (Layer B) semantics — incl.
   taint and the validity/drop rules (§1–§2).
2. Land zero-fill + padding/**inactive-payload** canaries **behind the old encoding hash**
   (so the canary can fail before identity changes) — CI-always.
3. Flip **whole-value identity slots** and projection `Whole` reads to field reads where
   the store already has `content_hash` (`:9831` — see §6, this is largely already a
   field read).
4. Move **arrays** to the second-epoch carried path; keep the `changed_words` fallback.
5. **Maps/records: preserve sort-at-finalize, OR gate the ordered-structure map/record
   carry as a SEPARATE sub-epoch with its own proof (Q1/Q2).** This is where the fold
   algebra + flat-memory dependency lives; it does **not** block gates 3–4.
6. Only then delete `hash_value_into` descriptor-walk arms truly subsumed by the flat
   proof, and the observation re-hash arms subsumed by field reads.

Gate 5 is a **dependency** on gate 2 (flat memory) + the Q1 structure — its force-copy
fixture cannot even be written until the fold algebra exists. Both committees review before
fold, per first-epoch discipline.

---

## 6. Cost model

### Expected wins, per trunk — with the overclaims subtracted

| trunk | today | after | banked? |
|---|---|---|---|
| blake3 at every alloc (23.4%) | full descriptor-walk digest per intern (`:1419`, `:11211`) | one flat-bytes stencil pass on materialize, or O(1) finalize of a valid carried midstate | **robust** |
| observation re-hashing (13.6%) | `projection_observation_hash` per read (`:9980`) | field load of stored `content_hash` for `Whole`; cached child-hash reads for projections | **robust, but smaller than stated — see below** |
| map re-canonicalization (29.1%) | re-canonicalize + re-hash every pair + sort + dedup per intern (`:10376`) | ordered incremental structure (Q1 b) OR sort-at-finalize (Q1 c) | **CONTINGENT — not banked** |

**`ProjectionPath::Whole` overclaim subtracted (codex P2).** `projection_observation_hash`
for `Whole` calls `canonical_word_hash_in_store` (`:9990`), which **already returns
`entry.content_hash` directly** for a store handle with matching schema (`:9831`). So the
`Whole` cell is *already a field-read wrapper*; the residual cost is
call/dispatch/schema-match, **not a full rehash**. The 13.6% observation trunk's win is
therefore the *projection* cases (`Field`/`MapGet`/`Tag`/`DocGet`/`TreePath`, `:9992`–
`10073`) plus dispatch shaving — real, but smaller than a naive "kill the rehash" reading.

**29.1% map win is contingent (opus ruling 6).** It only materializes once Q1 (ordered
structure) + §3 (flat memory) dissolve `changed_words` for nested children. Report the
66% as **"addressable trunks," not "removed by this proposal."** Re-flame after gates 2–4
to confirm the map trunk actually collapses before banking it — and decompose the 29.1%
first (see Q1): if the dominant cost is per-pair *re-hashing* (which stable child
identities + cached pair hashes kill) rather than the *sort*, even sort-at-finalize (Q1 c)
captures most of it, and the ordered-structure research may be unnecessary.

### Amdahl honesty (opus ruling 4/6)

Removing 66% of flame is **≈ 2.9× at best** — and less initially, since the 29.1% is
contingent. §1–§5 do **not** alone reach 50×. What remains:
- **per-INVOKE memo floor** — every source-level vix call interns molten args for the memo
  key (LINCHPIN #2). Identity-as-field makes the per-step cost a field read but does not
  remove the per-INVOKE intern boundary. Lever: **memo granularity in solver interiors** —
  keep interior iteration molten (tail-loop, landed), pay identity once per interned
  aggregate, not per step.
- **interp dispatch** — the residual after identity is free. Removed by the **JIT lane**,
  which identity-as-field is the *precondition* for: §2 Layer-A field read + §4 stencil let
  the JIT emit identity inline instead of a host-call that would dominate JIT'd code the way
  `Driver::spawn → compile` did (RESURRECTION JIT anomaly).

### The path to 50× (both legs load-bearing, not garnish)

1. **Identity-as-field** (this proposal, robust trunks first) removes the recompute wall —
   the existential precondition.
2. **JIT lane** turns residual interp dispatch into native code; identity is a field
   load / patched stencil, so the JIT actually pays.
3. **Memo granularity in solver interiors** — keep interior iteration molten; map identity
   optimized only *after* a map-heavy profile justifies the ordered/Merkle structure.

The claim is **not** that §1–§5 hit 50×. It is that they remove the one trunk that makes
50× unreachable *and* are the precondition that makes the JIT win real. That is the arc.

---

## Open questions

### Q1 — the map-hash construction (THREAT-MODEL REFRAME — Amos ruling 1)

**Identity hashes are supply-chain security artifacts**, not optimization-only checksums:
they become cachets (`.note.vixen.cachet`), content-keyed advisories, and cross-org cache
trade keys. **Collision resistance under adversarial content is a hard requirement** — vix
values include registry metadata, manifests, archives, and build outputs from potentially
hostile sources. My original recommendation (XOR/add commutative combine) is **withdrawn**:
codex is right that a public abelian-group sum of attacker-influenceable addends falls to
**Wagner's generalized-birthday attack** (even per-pair-hashed addends: the attacker
supplies enough pairs to solve the k-sum). Opus is right that the algebra must support
**O(~1) mutation including delete**. The three viable constructions, with costs:

- **(a) LtHash-class lattice multiset hash.** Proven (Facebook LtHash / homomorphic
  MSet-Hash lineage), true multiset semantics, O(1) add *and* delete via the lattice
  inverse. Cost: **~2KB carried state per molten map** (a wide vector over a lattice
  modulus) — heavy for molten scratch that churns per solve step. **Changes the taxon
  value-map canonical-encoding spec** (a new cryptographic construction with its own
  security assumptions, collision target, duplicate semantics, and adversarial test
  suite). *Only (a) touches the spec.*
- **(b) Ordered incremental Merkle / B-tree over canonical key order. — RECOMMEND.**
  A balanced tree keyed by `(key_schema, canonical key comparison / key hash with
  tie-break)`; each node holds `blake3` of its children; final identity is the **root
  hash**. Mutation (insert/overwrite/**delete**) updates **O(log n)** nodes; finalize is
  **O(1)** (read the root). **Standard Merkle security — no new cryptographic
  assumptions.** Crucially, being *ordered*, it **realizes the CURRENT sorted-canonical
  spec incrementally**: molten-fold == intern-fold (no re-sort at finalize), so it captures
  opus's soundness benefit (dissolves `changed_words` for maps) **without** the additive
  weakness. ~O(log n) small nodes of carried state — far lighter than (a). This is the
  recommendation.
- **(c) Sort-at-finalize (no map carry).** Keep `canonical_map_pairs`' sort; do not carry
  a map midstate. Zero new machinery, realizes the current spec exactly. Arrays,
  whole-value slots, and projection field-reads still pay off (they are independent — gate
  split, §5). Cost: map intern stays O(n log n), but **only the sort** — the per-pair
  *re-hashing* and *re-canonicalization* (likely the bulk of 29.1%) are already killed by
  stable child identities + cached pair hashes under §3. **Measure whether map-intern
  frequency even needs better than this before building (b).**

**Recommended sequence:** land (c) as the gate-2/gate-4 default (it needs nothing new),
re-flame (§6), **decompose the 29.1%** into re-canonicalize / re-hash / sort; build (b)
only if the *sort* residual alone still justifies it. Reserve (a) for a future
multiset-heavy profile that (b) cannot serve — and only with its own written crypto spec.
**Note:** (b) and (c) realize the current sorted-canonical value-map encoding; **only (a)
changes it.**

### Q2 — record carried-hasher × atomic variant switch

A linear blake3 midstate supports **append only** (arrays); it cannot do the **in-place
field/variant update** records need (you cannot subtract a field from a sequential
midstate). So the record carried hasher, if built, uses the **same ordered-structure
discipline as Q1(b)** over per-field child identities — a small Merkle/positional tree
keyed by field offset, root-hashed. A variant switch is then the atomic primitive of §3
(zero inactive payload → tag → write → fold the new variant's field contributions),
updating O(log fields) nodes, no double-count, no race. Under the gate split (§5) this is
part of the deferred map/record sub-epoch; arrays (linear append-midstate) and whole-value
slots do not wait on it. Open: whether records even need a carry, or whether flat-memory
whole-record hashing (§3) at intern is already cheap enough — decide by the same re-flame.

### Q3 — projections beyond `Whole`

`Whole` is *already* a field-read wrapper (`:9831`); the win there is dispatch shaving, not
rehash elimination (§6). `Field`/`MapGet` should read the **interned child's own
`content_hash`** (the `Access::Handle` arm already hashes a child's cached hash), not
re-canonicalize. `Tag`/`TreePath`/`DocGet` over scalars stay inline-cheap. Open: whether
large `DocGet`/`TreePath` sub-trees warrant a *per-projection* memo (identity-of-a-projection
as a cached fact) — recommend measuring after gates 2–4; the 13.6% (already smaller than
first stated) may largely be gone.

### Grounding correction (codex non-blocking note — folded)

This doc no longer cites `r[schema-identity.canonical-encoding]` for **runtime value-map
identity**. That taxon spec encodes `Kind::Map` **schema** identity (key ref then value
ref, `phon/rust/taxon/src/identity.rs:301`) — it does **not** define runtime map-*value*
canonicalization. The value-map canonical encoding is a **driver-level** spec
(`canonical_map_pairs`, `:10376`), and only Q1 construction (a) would change it; (b)/(c)
preserve it. Extending taxon into value canonicalization would be its own decision, not
assumed here.
