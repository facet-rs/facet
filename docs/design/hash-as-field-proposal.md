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
  solve-class run**; the perf lane is committing the flame notes — cite the committed
  paths once they land. **The stencil / hash-cost evidence IS committed and pinned:**
  `notes/blake3-stencil-spike.md` on `origin/blake3-stencil-microbench` (round-1/round-2
  tables + the post-lever residual projection, `VIX_HASH_WORKLOAD_COUNTS=1` ring
  harness) — §4 and §6 cite it directly and it is reproducible from that branch.

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
  the digest arrives already-finished from the molten carried state (§2) or a
  flat-memory blake3 pass (§4, `blake3` crate host-call — the stencil is retired). No
  layout change; we change *who writes it*.

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
   carried midstate if valid, else a flat-bytes blake3 pass (§4, host-call) over the canonical
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

## 4. The blake3 stencil — MEASURED DEAD as a perf lever

**Decisive finding (stencil spike round 2 — `notes/blake3-stencil-spike.md` on
`origin/blake3-stencil-microbench`).** The blake3-as-stencil direction this section
originally recommended is **retired.** It was measured, and it loses:

- The hand-written **AArch64 NEON stencil** loses to the `blake3` crate host-call path in
  **every** measured intern, fold, and batch shape — 2.2–3.3× *slower* for single interns,
  2.6× for the carried parent fold, 1.4–2.9× batched (spike note, Round 2 tables).
- The **zero-boundary inline-NEON** column (no FFI at all) *also* loses to the host-call
  crate path — so the loss is not the call boundary, it is that a hand-rolled
  single-compression NEON stencil is simply not the crate's optimized lane (the crate's SIMD
  win comes from platform-specific multi-input chunk batching a one-block stencil cannot
  reach). Even the **native crate ceiling** barely edges the host-call path (0.0114 vs
  0.0120 s/ring-16).

The IR-vocabulary reasoning below still *holds* (weavy IR has no arithmetic op set —
`weavy/src/ir.rs`, `MemoryOp`/`InitOp`/`AggregateOp`/`Control`; the `Add` in
`weavy/src/async.rs` is a toy; so blake3 arithmetic could only ever be a stencil, never
lowered ops). But the conclusion flips: **there is no reason to build the stencil at all.**
The `blake3` crate host-call is the fastest available path *and* the correctness anchor.

**Why this does not matter — the money number.** In the hash-as-field world (identity is a
field; the only residual hashing is `raw_new` intern payloads + carried folds), **all
hashing for ring-16 costs ~11 ms** (0.0114 s host-call, spike Post-Lever Residual
Projection) **against the current ~33,800 ms budget** — ~0.03%, rounding error. Hash
*computation* is not a trunk once identity is a field. The whole "make the hash faster"
axis (stencil, SIMD, IR-lowered compression) is therefore **closed**: the win is not in
hashing less, it is in not *building the input to* the hash (§6).

**Interp/JIT identity parity — preserved trivially.** Both lanes call the identical
`blake3` crate routine as an ordinary Rust call. One compression body ⇒ byte-identical
identity across interp and JIT, the same guarantee `finish_hash`/`blake3::Hasher` gives
today (`:740`). No stencil needed to secure this; the host-call *is* the shared body.

**Carried-midstate ABI is process-local only (codex non-blocking note, still binding).**
`blake3::Hasher` is a Rust type, not a portable cross-version memory contract. The carried
midstate (Layer B) is valid **only within a single process** — molten scratch, never
persisted, never sent cross-version. Persisted/cross-process identity is always the
*finalized* `ContentHash` (Layer A). This is unchanged by the stencil's retirement (the
carried hasher is still a `blake3::Hasher`, just always host-side).

### Const-eval: compile-time determinism only, NOT a solve-throughput lever

An earlier draft argued machine-side blake3 enables compile-time hashing of literals/static
schemas, and framed FFI as "costing four optimizations" including const-eval. **The perf
half of that argument is withdrawn by the measurement above:** since hash *computation* is
~0.03% of the budget, precomputing constant hashes at lowering time buys **nothing for solve
throughput**, and the "FFI precludes const-eval" drum **does not apply to hot-path perf** —
there is no hot-path hashing cost to eliminate.

Const-eval of static hashes may still have value for **compile-time determinism** (a fixed
schema's `ContentHash` being a genuine compile constant is cleaner for reproducibility and
for baking cachet coordinates), but that is a *determinism/ergonomics* argument, explicitly
**not** a throughput one. Marked clearly so no charter mistakes it for a lever: **do not
build const-eval-of-hashes for performance.** The FFI-boundary cost that *does* remain
load-bearing is the one in Q1(b) — inlining/fusion/register-residency of **collection
operations** (map/tree touches), which are real hot-path work; hashing is not.

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
- **Interp/JIT cross-lane differential** (§4): the shared `blake3` crate routine must agree in both lanes
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

### The core mechanism: eliminate the encode-and-allocate pipeline, not the hash

The measurement in §4 forces a correction to how this proposal describes its own biggest
win. The 23.4% "alloc → `raw_value_content_hash` → blake3" flame trunk **is not blake3.**
blake3 computation is ~11 ms/ring-16 against a ~33,800 ms budget (§4) — rounding error. The
23.4% is the **canonical-encoding construction + allocation that *feeds* the hash**:
`alloc` builds a fresh `Vec<u8>` canonical encoding of the value, then walks it. The cost is
the *encode* and the *allocate*, not the digest over the result.

So **hash-as-field wins by eliminating the encode-and-allocate pipeline, not by hashing
less** — and the zero-padding / flat-memory law (§3) is what makes it exact:

> Under flat-memory identity **there is no encoding step.** You hash the bytes that
> *already exist* in the value's memory (canonically zeroed, weavy-declared layout). No
> `Vec` is allocated to hold a canonical encoding; no descriptor walk re-serializes the
> value; the digest reads live value memory in place.

That is the mechanism behind the 23.4% collapse — and it is why §3 is load-bearing for
*perf*, not only for the second-epoch soundness argument (§2): flat-memory removes the
allocate-and-encode round-trip that the flame actually charges. The blake3 pass at the end
was never the cost.

### Expected wins, per trunk — with the overclaims subtracted

| trunk | today | after | banked? |
|---|---|---|---|
| alloc→encode→blake3 (23.4%) — the cost is **encode + allocate**, not the hash | `alloc` builds a fresh `Vec` canonical encoding then walks it (`:1419`, `:11211`) | flat-memory: hash live value bytes in place, **no encode, no alloc** (§3); or O(1) finalize of a valid carried midstate | **robust — mechanism is pipeline removal, §3** |
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

### Amdahl honesty, and what the residual actually is (opus ruling 4/6, sharpened by §4)

Removing 66% of flame is **≈ 2.9× at best** — and less initially, since the 29.1% is
contingent. §1–§5 do **not** alone reach 50×. The measurement sharpens *what remains*, and
it is the important correction: **the residual after gates 1–2 is memo / demand / interp
overhead — NOT hashing.** Hashing is solved by the gates (identity is a field; the leftover
digest work is ~0.03% of the budget, §4). So the remaining ~order of magnitude to the 50×
bar is entirely **execution-model**:
- **per-INVOKE memo floor** — every source-level vix call interns molten args for the memo
  key (LINCHPIN #2). Identity-as-field makes the per-step cost a field read but does not
  remove the per-INVOKE intern boundary. Lever: **memo granularity in solver interiors** —
  keep interior iteration molten (tail-loop, landed), pay identity once per interned
  aggregate, not per step.
- **interp dispatch** — the per-op match/dispatch loop. Removed by the **JIT lane**, for
  which identity-as-field is the *precondition*: §2 Layer-A field read lets the JIT emit
  identity as an inline load instead of a host-call into the digest machinery that would
  otherwise dominate JIT'd code the way `Driver::spawn → compile` did (RESURRECTION JIT
  anomaly). (Note: the JIT emits a **field load**, not a blake3 stencil — §4 retired the
  stencil; identity is already computed and stored by the time the JIT reads it.)

### The path to 50× — hashing is solved by the gates; the rest is execution-model

State it plainly, because the measurement licenses it:

1. **Identity-as-field (this proposal) solves hashing.** Gates 1–2 remove the
   encode-and-allocate pipeline (23.4%) and the observation rehash (13.6%); the dense-index
   transform (§7) or Q1 handles maps (29.1%). After that, hash computation is negligible and
   there is **nothing left to optimize on the hashing axis** — no stencil, no SIMD, no
   IR-lowered compression (§4). This axis is **closed**.
2. **JIT lane** turns residual interpreter dispatch into native code — now unblocked,
   because identity is a field load rather than a host-call barrier. **Load-bearing.**
3. **Memo granularity in solver interiors** — keep interior iteration molten; pay identity
   once per interned aggregate, not per step; reshape the solver `State` (§7) so the trail is
   an array append. **Load-bearing.**

The honest claim: **hashing is done after the gates; the remaining ~order of magnitude to
50× is JIT + memo-granularity — an execution-model result, to be *measured* not assumed.**
Re-flame after gates 1–2 to confirm the residual is dispatch/memo (as the spike projects),
then the JIT and granularity legs carry the rest. This proposal is the precondition; it is
not the finish, and it is no longer even the *hashing* finish disguised as one — the hashing
finish is the gates, full stop.

**Orthogonal solver-specific win — the dense-index transform (§7).** The 29.1% map trunk
analysis above assumes the solver's `State` stays *map-shaped*. It need not: the solver's
key universe is **open-but-monotone** (keys discovered lazily during solve, only ever
growing), which admits a transform to a dense-array `State` via a **dense-id interner** that
the **existing array carried-hasher already handles** — no map hash construction (Q1) on the
hot path at all. This is potentially a *large* solver-specific
win that is **orthogonal to the gate sequence** (it does not wait on flat-memory or the
weavy-native tree); §7 develops it, and it may make the map trunk's collapse a matter of
*not building a map* rather than of hashing one faster.

---

## 7. Monotone-interned collections: the dense-index transform (Amos probe)

The map-hash discussion (Q1) implicitly treats the solver's `State` as an **open-universe
map** — arbitrary keys arriving over time, needing an ordered structure to hash. That
over-states the problem *and* an earlier draft of this section over-corrected it. The
precise claim (Amos correction):

**The solver's key universe is OPEN-BUT-MONOTONE, not closed.** rodin's real workload is
**gradual discovery** — sparse rows are demanded lazily *during* solve; keys (packages, and
per-package candidate versions) appear as the search touches them, and the set only ever
**grows** (monotone — nothing is un-discovered). Today's harness pre-composes the whole
Index up front, but that is a **measurement rig**, not the workload: it front-loads
discovery so the solver can be timed in isolation. Designing to the rig ("closed universe,
assign all ids up front") would bake in an artifact of the benchmark. The transform must
survive **lazy, monotone discovery** — and it does, via an interner rather than a pre-pass.

### The transform — a dense-id interner, not a pre-pass

The dense route works on a monotone universe via a **dense-id interner**: **first touch of
a key allocates the next `u32`** (`package → id`, `(package, version) → id`), and SOA arrays
**grow at the tail** to accommodate it. Discovery of a new package appends; no key is ever
removed. The solver `State` is then **SOA arrays + bitsets** indexed by dense id:
- a domain per package = a bitset over its candidate-version ids;
- assignments / decision levels / watched literals = flat arrays indexed by package id.

For identity, this is decisive **and it is the array carry's *best* case**: append. A
dense-array `State` is **array-shaped**, so the **existing array carried-hasher**
(`start_array_element_hasher` `:776`, `update_array_element_hash` `:787`,
`finish_array_element_hash` `:809`) does the identity work directly, and a newly-discovered
key is a **tail append** — exactly the consuming-move/append path the array carry already
proves (§2, `changed_words` never fires for stable-identity elements). **No Merkle tree, no
map-hash construction, no Q1 on the solver hot path**, and the monotone-growth pattern is
the one the carry is fastest at.

Structural sharing survives: a successor `State` flips one bitset / one array slot (or
appends one on discovery), so the array carry re-folds a single element — the same
O(1)-per-mutation the trail loop already gets (the bitset word is an inline scalar, the
cheapest possible fold). This is why the dense route can beat even the persistent tree
(Q1(b)) for the *solver* workload: the tree gives O(log n) per State; the dense array gives
O(1) per changed-or-discovered domain.

### The load-bearing rule: ids are solve-local representation, NEVER identity

Because ids are assigned **by discovery order**, and discovery order depends on the search,
**the same package can get a different id in two different solves.** Therefore:

> **Dense ids are a solve-local representation. They are NEVER identity, and nothing that
> crosses the solve boundary may be keyed by them.**

- **Interior to one solve**, ids are the fast index and the array-carry element positions.
  Machine determinism makes them reproducible *within* that solve (same discovery order →
  same ids), so a `State`'s array-carry digest is well-defined and stable during the solve.
- **Crossing the solve boundary** — memo keys, receipts, `.vix-cas` keys, learned/warm
  facts — everything stays keyed by **content identity** (`ContentHash` over the *value*:
  the package coordinate, the version, the premise set), **never by id.** Ids do not survive
  across differing solves and must never be leaned on there.
- The **warm-facts spec already complies**: facts are keyed by **premises** (content), not
  by any solver-interior index — so the fact store is already id-free at its boundary. This
  is the pattern to preserve everywhere: id-fast inside, content-keyed at every edge.

This is the same discipline as HandleTier staying out of identity (§1): a representation
detail that speeds the interior must be invisible to the value's name.

### Scope split for Q1

This creates a clean scope split the charter should hold:
- **Monotone-interned hot paths (the solver `State`)** → the **dense-id-interner route**.
  Q1's tree is *not* on this path. Expressible in **vix today** (interned `u32` ids + flat
  arrays + bitsets over declared layouts — the same idioms rodin already reaches for);
  **experiment chartered on the tier-A lane.** Ids stay solve-local (the rule above).
- **Genuinely open, non-interned maps** (registry metadata, manifests, arbitrary Doc maps
  whose keys are unbounded content, not drawn from an interner) → **Q1(b)'s weavy-native
  ordered tree** remains the answer.

The two are not in tension: dense-interner is the *specialization* for a monotone key
domain fed by an interner; the tree is the *general* structure for content-keyed maps. The
charter picks per call-site by whether the keys come from an interner.

### Future compiler transform — own the key→index layer as a pass

Today the dense-index transform is *hand-written* (the solver author interns ids). The
destination is to make it a **checker/lowerer pass** — but note the corrected trigger: not
"provably-closed key universe" (which the gradual-discovery workload does *not* satisfy)
but **"monotone-interned key domain."** Recognize a map whose keys **come from an interner**
(monotone-allocated ids, no deletion) and **lower it to dense arrays automatically** —
route the interner, rewrite `map_get`/insert into array index/bitset ops, route identity
through the array carry, **and enforce the solve-local-id rule** (the pass must keep ids out
of anything that crosses a memo/receipt/fact boundary, re-keying those by content). "**Own
the key→index layer**" as a compiler responsibility, not a solver-author idiom — the same
philosophy as the rest of the proposal (the machine owns what is hand-rolled today), applied
to collection *representation selection*. Future work; the hand-written experiment is the
prototype of this pass, not the endpoint.

### Cost-model placement

Fold this into §6 as a **solver-specific trunk win orthogonal to the gates**: if the dense
transform lands (even hand-written, on tier-A), the 29.1% map trunk on the *solver* workload
is addressed by **eliminating the map**, not by hashing it — independent of gate 1
(flat-memory) or gate 5 (weavy-native tree). It does not reduce the general map-hash work
(open-universe maps still need Q1), but the solve-class flame that motivates this whole
proposal is dominated by the *solver*, so the dense route may be the single largest
practical win in the document — and the cheapest to reach, since it needs no new epoch
machinery, only the existing array carry over a re-shaped `State`.

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

**Scope first (see §7):** this question is about **open, non-interned maps** only. The
solver's `State` has an **open-but-monotone, interned** key domain (keys discovered lazily
during solve) and takes the **dense-id-interner route** (§7) — the existing array
carried-hasher, no tree, no Q1 on that hot path, ids strictly solve-local. The constructions
below are for genuinely content-keyed maps (registry metadata, arbitrary Doc maps).

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

  **The strongest argument for (b) is STRUCTURAL SHARING (Amos's data-structure probe),
  and it is currently undersold.** Solver `State`s differ by **one domain at a time** — a
  successor State mutates a single package's domain and leaves every other subtree
  untouched. A **persistent** (functional) digest-carrying B-tree means successive States
  **share both memory *and* identity computation** for every unchanged subtree: the path
  to the changed node is copied (O(log n) nodes, each re-hashed), and the untouched
  siblings are retained by pointer *with their subtree digests intact* — no re-hash, no
  re-alloc. This is spike-C's carried-hasher philosophy generalized from a linear
  accumulator to the **structure itself**: the trail becomes cheap *by construction*,
  because the identity of a State is a fold over subtree digests that are already computed
  and already shared. (a)'s flat ~2KB accumulator cannot share — every State carries its
  own vector — and (c) recomputes the whole sort per intern; only the persistent ordered
  tree makes both the memory and the identity of a 10,000-domain State differ from its
  predecessor in O(log n), which is the trail loop's actual access pattern.

  **CHARTER REQUIREMENT (Amos): the persistent Merkle B-tree must be WEAVY-NATIVE.** Its
  nodes are **descriptor-declared values** (weavy-declared layouts, §3's zero-padding law
  applies to them directly), and its operations — insert / get / fold / finalize / the
  path-copy on mutation — are **lowered to weavy IR and JIT-emitted**. (The node blake3
  hash itself stays a `blake3` crate host-call, not a stencil — §4 measured the stencil
  losing; the win here is the *collection ops* in weavy IR, not the hash.) They are **NOT
  Rust host ops.** The
  rationale is the destination of the whole proposal, not a micro-optimization:

  > vix maps today are host ops — every touch (`alloc_map`, `map_get`, `canonical_map_pairs`)
  > crosses into opaque Rust that **decodes** canonical payload bytes, manipulates, and
  > **re-encodes**. The crossing itself costs ~2ns, but the **opacity is fatal**: the JIT
  > can never inline or fuse across the FFI boundary, never keep a hot map in registers,
  > never elide the decode/encode round-trip. *We can optimize weavy codegen but we can
  > never optimize FFI.*

  Therefore **host-side map ops are explicitly the INTERIM implementation.** The
  destination is **machine-side collections over declared layouts**: the typed-collections
  epoch (V1/V3, landed) laid the *descriptors*; the *operations* must now follow them onto
  the machine side. A weavy-native persistent B-tree is not just the fastest map hash — it
  is the vehicle by which vix's core collection stops being an FFI black box the JIT must
  treat as a barrier. This reframes Q1(b) from "the safe map-hash construction" to "the
  first machine-side collection," and it is why (b) is the recommendation even though (c)
  is cheaper to reach: (c) keeps maps host-side forever.

  **Dependency this puts on the critical path — weavy IR/codegen vocabulary for
  pointer-chasing tree ops.** A persistent tree is *pointer-chasing* (node → child pointer
  → child), and node-sharing across States means those pointers are **owned/shared
  references into a persistent arena**, not flat offsets. The **stencil spike's finding
  (committed: `notes/blake3-stencil-spike.md` on `origin/blake3-stencil-microbench`,
  round-1 and round-2 infrastructure findings) — that "the typed task vocabulary has no
  principled pointer/capability argument story" (the bench had to pass raw pointer words in
  frame slots, "acceptable for the spike but not a real machine ABI for weavy-native
  collections"), and that the task JIT "has no custom consumer-op extension point"** — is
  therefore now **on the critical path for Q1(b)**, not a side concern. (The spike's own
  closing line names this as "the durable proposal input.") Building a machine-side tree requires weavy to express:
  (i) a node pointer/handle as a first-class IR value with declared provenance, (ii)
  load-through and store-through a node reference, (iii) structural-share retain/release
  of a subtree (the persistent-arena refcount), and (iv) the path-copy-on-write shape. This
  vocabulary gap gates the weavy-native tree; the charter must sequence it *before* gate 5
  (map/record carry) and fund it as part of that sub-epoch, not assume it exists. Until it
  does, (c) (host-side sort-at-finalize) is the honest interim — which is exactly why the
  gate split (§5) keeps the array/whole-value wins independent of it.
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

**Small-map hybrid (the representation is invisible to identity).** Per-package domains
are *tiny* — a handful to a few dozen candidate versions — so a full B-tree per map is
overkill at that size. Use a **flat sorted-vec representation below ~N entries and the
persistent tree above** (the usual small-map inline optimization). The hard constraint:
**the digest algebra must be identical across both representations** — the same canonical
fold over the same canonical key order — so that a map that grows across the threshold, or
two maps of the same content stored in different representations, produce the **byte-identical
`ContentHash`**. Identity must not be able to observe which representation is in use;
the threshold is a memory/perf decision only, never a canonicalization decision. (This is
the map analogue of the array/scalar inline-word exemption in §1: representation varies,
identity does not.) Force-copy differential (§5) covers it directly — force a small map to
the tree representation and assert the digest is unchanged.

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
