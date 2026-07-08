# HASH-AS-FIELD / memory-identity (the second epoch)

Status: **design, committee-gated** (implementation not authorized by this doc).
Author lane: docs-only (`hash-as-field-proposal` worktree). Coordinates with the
standing perf lane, which is landing tactical levers in the same `driver.rs`
territory — this document touches no code.

Required grounding (read before relitigating anything here):
- `~/oss/facet-cc/RESURRECTION.md` — "The ratified design", the IDENTITY AMENDMENT
  (2026-07-08), the ONE-hash-break epoch, the carried-hasher lever.
- `capabilities-ambient-vs-materialized.md` — the *zero-padding law* dictation
  ("Padding ANSWERED: CANONICAL ZERO, ALWAYS"), the blake3/flat-memory direction,
  and the second-epoch authorization addendum.
- `vix/docs/content/redesign/2-hashing-flesh.md` — the hash-as-field thesis in its
  first (pre-blake3, "flesh") form; the "get the hash scheme right and flesh's job
  gets simpler" argument.
- `vix/src/machine/driver.rs` — the live hash sites (cited inline below).

---

## 0. The existential framing

A native reference solve runs the *identical* index at **~95,000×** vix on
solve-class workloads (perf-lane flame, solve-class run). Of vix's solve time,
the flame attributes **~66%** to *recomputing identity bytes that the machine
already knew*:

| trunk | flame share | live site |
|---|---:|---|
| blake3 at every allocation | **23.4%** | `ValueStore::alloc` recomputes a full-value digest on every intern — `driver.rs:1407`, hashing at `:1419` |
| map re-canonicalization per intern | **29.1%** | `alloc_map` → `canonical_map_pairs` re-canonicalizes, re-hashes every key+value, sorts, dedupes from scratch — `driver.rs:1466`, `:10376` |
| observation re-hashing | **13.6%** | `projection_observation_hash` re-derives a canonical digest for every demanded read — `driver.rs:9980` |

This is not a constant-factor tax; it is *repeated work over immutable data*. Every
one of these three trunks recomputes a value that is, by the machine's own
invariants, **write-once**: interned store entries are immutable, so their identity
is fixed the instant they exist. The proposal is to stop recomputing it and instead
**store it as a field** — computed once, at the write, carried incrementally while
molten, read as a plain field load forever after.

The 50× bar (RESURRECTION, "acceptance bar RELAXED to ~50× vs Rust") is the target.
Killing the 66% identity-recompute trunk is the *existential* precondition; §6 is
explicit about what remains after and the honest path from there.

---

## 1. The identity slot

**Claim.** Content identity is a *property of a value*, not a computation over it.
Every non-inline value class gets a descriptor-reserved slot that holds its
identity. The slot's *representation* differs by lifecycle stage but its *meaning*
(the blake3 content hash of the canonical value) does not.

### Per value class

- **Interned store entries** — already carry `content_hash: ContentHash` in
  `StoreEntry` (`driver.rs` store entry struct; `ContentHash = [u8;32]`). Today it
  is *recomputed* at `alloc` (`:1419`) then cached. Under this proposal the cached
  field *is* the identity slot and the recompute path is deleted: the digest arrives
  already-finished from the molten carried hasher (§2) or the blake3 stencil (§4).
  No layout change — this class already has the slot; we change *who writes it*.

- **Molten (carried) values** — `MoltenEntry` today holds
  `carried_array_hash: Option<CarriedArrayHasher>` (`driver.rs:1141`), and
  `CarriedArrayHasher` wraps a live `blake3::Hasher` (`:1068`). This is the identity
  slot *for arrays only*, in its incremental (not-yet-finalized) representation. The
  proposal **generalizes the slot to every molten class** — `MoltenValue::{Record,
  Map, ArrayWords}` (`:1145`) — so the molten identity slot is
  `Option<CarriedHasher>` uniformly, carrying the in-progress digest state, promoted
  to a finished `ContentHash` at intern.

- **Inline scalars — exempt.** `schema_is_inline_word` values (`driver.rs:2037`) are
  their own identity: the word bytes *are* the canonical encoding, memcmp-comparable,
  no digest needed. `hash_value_into`'s `Access::Scalar` arm already hashes raw word
  bytes directly (`2-hashing-flesh.md` records this as the pre-existing POD-flat fast
  path). Reserving a slot for these would be pure waste; they stay slot-free. This is
  the one class boundary the design draws hard.

### Representation: molten vs interned

| stage | slot type | meaning |
|---|---|---|
| molten, mutable | `Option<CarriedHasher>` | in-progress incremental digest state (blake3 midstate), carried across mutations |
| interned, frozen | `ContentHash` (`[u8;32]`) | the finalized digest — the value's permanent name |

The transition molten→interned is the **finalize** (`finish_hash`,
`driver.rs:740`): the carried midstate is closed out, the descriptor slot flips from
"carried" to "final", and the value becomes immutable. After that the slot is
read-only forever.

---

## 2. Write-once, read-forever semantics

The lifecycle of the identity slot, stated as four rules:

1. **Computed at intern** — when a value freezes, its digest is finalized *once*.
   For a molten value that carried its hasher, finalize is `finish_hash` over the
   already-fed midstate (O(1) in value size). For a value materialized flat (e.g. a
   facet-bridge copy-in, §3), it is one blake3-stencil pass over the canonical bytes
   (§4). Either way: **one** digest per value, at the write.

2. **Carried incrementally on molten mutation** — the array case is *already proven*
   in the tree: `start_array_element_hasher` (`:776`) seeds domain + element schema,
   `update_array_element_hash` (`:787`) folds each appended element's hash, and
   `finish_array_element_hash` (`:809`) closes with the length. This is the
   spike-C "incremental append-hash" lever (RESURRECTION: "7,300× → 16.8× SHA-256"
   on the store-append path). The proposal **generalizes the carried-hasher discipline
   to maps and records**: a molten record mutation folds the changed field's
   (offset, new-child-hash) into the midstate; a molten map insertion folds the
   (key-hash, value-hash) pair. The trail loop that dominates solve is array-shaped,
   so arrays are the measured 50× lever; maps/records extend the *same* mechanism to
   the map re-canonicalization trunk (§0's 29.1%).

3. **Invalidated never** — post-intern the value is immutable, so the slot is never
   invalidated, only read. Molten mutation does not invalidate a *finished* hash; it
   updates a *carried midstate* that was never finalized. The one existing invalidation
   is honest and stays: `intern_molten_word` drops the carried hash when interning
   *changed* a child word (`driver.rs:2135` `changed_words → carried_hash = None`),
   forcing a recompute for that array — because the carried midstate was fed the
   pre-interned child words. The map/record generalization must preserve this rule
   (fold post-intern child identities, or drop-and-recompute on change).

4. **Read as a field load** — an interned value's identity is a struct field read,
   not a hash. This is the payoff that makes the JIT lane (§6) tractable: the JIT can
   emit a plain load from the descriptor-reserved offset instead of orchestrating a
   digest. The observation trunk (§0's 13.6%) collapses to this: `projection_observation_hash`
   (`:9980`) for a `Whole` projection should read the entry's stored `content_hash`
   field, not call `canonical_word_hash_in_store` — the field is already the answer.

---

## 3. Zero-padding law integration (Amos-directed)

`capabilities-ambient-vs-materialized.md` settles the ABI question this proposal
depends on: **weavy-declared layouts mandate padding == 0, no exceptions.** Rationale
(verbatim from the dictation): fresh pages are zero; hash-at-intern already touches
every byte so zeroing rides hot cachelines; memcpy preserves canonicity; the one
recurring tax is enum variant-switch slack-zeroing (bounded). C/Rust can't mandate
this (they don't own every writer); weavy owns every writer by construction.

What canonical-zero padding *buys* this proposal — the two enabling properties:

- **flat-bytes blake3** — the identity of a value becomes blake3 over its raw memory
  representation, no descriptor-walk. Today `hash_value_into` (`driver.rs:11211`)
  *walks* the descriptor (`Access::Record` recurse-per-field, `Access::Enum`
  recurse-per-variant, `Access::Array` recurse-per-element) precisely because it
  cannot trust padding bytes. With padding canonically zero, a leaf-flat value is one
  contiguous blake3 pass — the stencil in §4.
- **memcmp equality** — two canonically-zeroed values are equal iff their bytes are
  equal, so dedup can shortcut and the differential oracle (§5) is bit-exact.

**Zero-init obligations** the law imposes (the write-side contract):
- allocation zero-fills (fresh pages already are; the obligation is to *not* leave
  molten scratch un-zeroed before it becomes hashable);
- frame slots zero on entry (the weavy `Init` IR already has `Zero { offset, size }`
  — `weavy/src/ir.rs`, the InitOp family);
- **enum variant-switch slack** — when a sum type switches to a smaller-payload
  variant, the slack bytes must be re-zeroed (the bounded recurring tax the dictation
  named). This is a weavy lowering obligation on variant writes.

**Debug padding-canary at intern.** Under `debug_assertions`, at the finalize
boundary, assert every declared padding byte of the interned value is zero before
sealing the digest. This catches a missed zero-init obligation *at the value that
would be mis-identified*, not three demands later. (Note the RESURRECTION gotcha:
canaries must not be *inside* `debug_assert!` side-effect position — assert a
separately-computed predicate.)

**The facet-bridge canonicalization boundary.** The zero-padding law governs
weavy-declared layouts *only*. facet-*discovered* values (rustc dictates layout;
padding is whatever the Rust ABI left) canonicalize **at the bridge**: the copy-in
re-layouts into weavy-declared form and *mints* the guarantee there. So the identity
slot for a bridged value is computed on the canonical (weavy-side, zeroed) copy, never
on the raw Rust bytes. This keeps the law's boundary exactly where RESURRECTION's
ratified design put the facet bridge (post-V10, additive, hash-neutral).

---

## 4. The blake3 stencil

**Answered from copypatch's actual capabilities.** copypatch is copy-and-patch: it
"runs and patches bytes — it never encodes an instruction or interprets a" stencil
(`copypatch/src/lib.rs`). The arithmetic of a stencil lives in a **host-compiled
native code body**, patched with immediates and stitched into the JIT stream. weavy's
IR (`weavy/src/ir.rs`) is a *memory/aggregate/control* IR — `MemoryOp`, `InitOp`,
`AggregateOp`, `Control` — it has **no arithmetic/bitwise op vocabulary** (no
add/xor/rotate at the IR level; the `Add` in `weavy/src/async.rs` is a toy demo op).

This resolves the "needed IR ops (rotate at minimum)" question **by reframing it**:

> **No new arithmetic IR ops are required.** blake3's compression function (the
> add/xor/rotate G-mixing) does **not** get lowered to vix/weavy IR. It stays inside
> a **stencil body** compiled from the `blake3` crate by the host toolchain — `-O by
> construction`, exactly copypatch's value proposition. The only IR surface is an
> `Intrinsic` node (`weavy/src/ir.rs:39` `Intrinsic(Intrinsic)`) that the JIT patches
> with the input pointer/offset, output-digest pointer, and length immediate. Rotate,
> xor, and modular add never appear as vix ops — they are opaque native instructions
> inside the pre-compiled stencil.

**IR orchestration.** The identity computation the JIT *does* orchestrate is:
(a) obtain the flat canonical bytes (a `ScalarRun`/`Move` region under the zero-padding
law), (b) invoke the blake3-compression intrinsic stencil over that region, (c) store
the finalized digest into the identity slot (a `ScalarCopy` to the reserved offset).
All three are existing weavy IR shapes; only (b) is new, and it is an intrinsic, not
an op-set expansion.

**Interp-lane callability.** Trivially satisfied and *load-bearing for correctness*:
the interpreter calls the identical native `blake3` compression routine as an ordinary
Rust function call; the JIT patches the same routine as a stencil. Because both lanes
share one compiled compression body, identity is **byte-identical across interp and
JIT by construction** — there is no "the JIT hashes differently" failure mode. This is
the same guarantee `finish_hash`/`blake3::Hasher` gives today (`driver.rs:740`),
carried forward into the stencil world.

**Stencil inventory.** One core compression stencil (blake3 G-mix + finalize) covers
the flat-bytes path. The carried-hasher path (§2) needs a *midstate-update* stencil
(fold one 64-byte block into a carried state) — this is the copypatch-able primitive
behind incremental molten hashing. Both live beside the existing weavy stencils
(`weavy/stencils/{async_ops,hostcall,task_ops}.rs`).

---

## 5. The second epoch

RESURRECTION IDENTITY AMENDMENT (2026-07-08): *"the epoch's encoding-hashes are NOT
sacred. If canonical-zero-padding + flat-memory hashing proves viable, identity
migrates to canonical-memory hashing as a SECOND sanctioned epoch — its own break,
own gates, committee-ratified."* This section is the migration plan for that break.

**What changes.** The first epoch (the V3/V1/V2 break already on `rodin`) hashes
**canonical payload *encodings*** — `hash_value_into` walks the descriptor and hashes
a domain-separated, field-by-field *encoding* of the value. The second epoch hashes
**canonical *memory*** — blake3 over the zeroed flat bytes of the weavy-declared
layout. Same algorithm (blake3), different input. Every content hash in the store
changes value.

**What breaks.** Everything keyed by content hash: `ValueStore::by_content` dedup keys,
memo keys (`fn_hash` × canonicalized-arg identities), `.vix-cas` store keys, and any
`ReadObservation`/projection hash. Within a single process/build these all rederive
consistently — the break is only observable across the epoch boundary.

**Bridges — none needed.** Per RESURRECTION's ratified design, "*no cross-process
persistence exists yet, so intermediate breaks on the branch are free*." The only
persistent dependent is `.vix-cas`, which is regenerable by construction (it is a
memo cache, not a source of truth). So: **no compatibility shim, no dual-hashing
window, no migration of stored artifacts** — the second epoch is a clean cutover, same
posture as the first. If `.vix-cas` from a prior epoch is present, it is treated as
cold (miss-and-recompute), not migrated.

**How the oracles verify the new identity.** Two standing differentials, both already
in the machine's vocabulary:
- **Force-copy differential** (`VIX_FORCE_MOLTEN_COPY`, RESURRECTION "corpus-wide
  differential as standing guard"): run every corpus fixture with molten reuse forced
  off. Under canonical-memory identity, the reuse and force-copy paths must produce
  **byte-identical** content hashes — memcmp equality (§3) makes this exact, not
  approximate. Any divergence is a canonicalization/zero-padding bug caught at the
  value.
- **Interp/JIT cross-lane differential** (§4): the shared compression stencil must
  produce identical digests in both lanes over the corpus. This is the guard that the
  stencil path and the interp path agree.
- **First-vs-second-epoch structural check** (transitional, then deleted): assert that
  *dedup topology* is preserved — two values equal under encoding-identity are equal
  under memory-identity and vice versa (equality is invariant even though the hash
  bytes change). This proves the epoch swap changed *names*, not *equivalence classes*.

**Gate sequence.** (1) zero-padding law lands in weavy layouts + zero-init obligations
+ debug canary (committee-ratified with the layout work, per the dictation). (2)
blake3 flat-bytes stencil + intrinsic, interp-lane parity green. (3) carried-hasher
generalization to maps/records, force-copy differential green. (4) flip identity input
from encoding-walk to flat-memory; both differentials green corpus-wide; structural
epoch check green. (5) delete `hash_value_into`'s descriptor-walk arms that the flat
path subsumes; delete the observation re-hash path in favor of the field read. Both
committees review before fold, as with the first epoch.

---

## 6. Cost model

### Expected wins, per flame trunk

| trunk | today | after | mechanism |
|---|---|---|---|
| blake3 at every alloc (23.4%) | full descriptor-walk digest per intern (`:1419`, `:11211`) | one flat-bytes stencil pass on materialize, or O(1) `finish_hash` of the carried midstate | §2 rule 1 + §4 stencil |
| map re-canonicalization (29.1%) | re-canonicalize + re-hash every key/value + sort + dedup from scratch per intern (`:10376`) | fold (key-hash,value-hash) into carried midstate on molten insertion; intern finalizes | §2 carried-hasher generalized to maps |
| observation re-hashing (13.6%) | `canonical_word_hash_in_store` per demanded read (`:9980`) | field load of the stored `content_hash` for `Whole`; cached child-hash reads for projections | §2 rule 4 |

If the three trunks collapse as designed, ~66% of solve time is removed at the root —
the identity-recompute wall. The remaining question is what that leaves.

### What remains after (the honest accounting)

Killing the recompute trunk does **not** by itself reach 50×. What is left:

- **Memo-per-demand overhead.** Every source-level vix call is an INVOKE demand
  boundary that interns molten args for the memo key (RESURRECTION LINCHPIN #2). Even
  with free identity, the intern-args-per-call structure has a floor. The lever is
  **memo granularity in solver interiors**: the solve interior is molten (doctrine),
  so interior iteration should stay molten (the tail-loop feature, already landed) and
  *not* pay a memo key per step. Identity-as-field makes the per-step cost a field
  read; it does not remove the per-INVOKE intern boundary.
- **Interp dispatch.** With identity free, the residual is interpreter dispatch
  overhead — the per-op match/dispatch loop. This is what the **JIT lane** exists to
  remove. RESURRECTION records the JIT was *2.58× slower* than interp due to
  per-spawn recompilation (root-caused; compile-cache fix landed). Identity-as-field
  is a *precondition* for the JIT lane paying off: §2 rule 4 + §4 make identity a
  patchable stencil + field load the JIT can emit inline, instead of a host-call into
  the digest machinery that would dominate JIT'd code the way `Driver::spawn →
  compile` did.

### The path to 50×

1. **Identity-as-field** (this proposal) removes the 66% recompute wall — the
   existential precondition.
2. **JIT lane** turns the residual interp-dispatch cost into native code; identity is
   now a field load / patched stencil, not a host barrier, so the JIT actually pays
   (unblocks the anomaly class RESURRECTION named).
3. **Memo granularity in solver interiors** — keep interior iteration molten
   (tail-loop), pay identity once per interned aggregate not once per step, so the
   INVOKE-boundary floor stops dominating.

The claim is *not* that §1–§5 alone hit 50×. The claim is that they remove the one
trunk that makes 50× unreachable *and* they are the precondition that makes the JIT
lane's win real rather than swallowed by digest host-calls. That is the honest arc.

---

## Top-3 open questions

1. **Carried-hasher generalization to maps: fold-order vs sort-at-finalize.**
   Arrays carry cleanly because append is order-preserving (`update_array_element_hash`,
   `:787`). Maps are *canonicalized by sort* (`canonical_map_pairs` sorts by key-hash,
   `:10404`) — a carried midstate fed in insertion order cannot be finalized to the
   sorted-canonical digest without either (a) re-sorting at finalize (partially
   defeating the carry) or (b) maintaining an order-independent commutative fold (e.g.
   a homomorphic combine of per-pair hashes that is insertion-order-invariant). **Recommendation:**
   option (b) — an order-independent per-pair hash combine (XOR/add of pair digests
   into the midstate, length-and-domain-sealed at finalize), so molten map mutation is
   O(1) and finalize needs no sort. Risk: order-independent combines are weaker against
   adversarial collisions; since these are *content* hashes over trusted machine-owned
   data (not adversarial input), blake3-per-pair + commutative combine is acceptable —
   but the committee should rule, because it changes the canonical-encoding spec
   (`r[schema-identity.canonical-encoding]`) for maps.

2. **Does the zero-padding law's variant-switch slack-zeroing interact with the carried
   record hasher?** A molten record whose enum field switches variants must re-zero
   slack (§3) *and* re-fold the changed region into its carried midstate. If the
   slack-zero and the fold race or double-count, identity corrupts. **Recommendation:**
   make variant-switch a single atomic molten mutation that (zero slack → fold the
   whole variant region), never two folds; add a force-copy differential fixture that
   exercises a variant shrink specifically. Open because the carried-record hasher does
   not exist yet — its mutation granularity is a design choice this proposal defers to
   the layout committee.

3. **Observation projections beyond `Whole`: field read vs recompute.** §2 rule 4
   collapses `ProjectionPath::Whole` to a field load, but `Field`, `MapGet`, `Tag`,
   `DocGet`, `TreePath` projections (`driver.rs:9992`–`10073`) hash a *sub-value* that
   has no top-level identity slot. **Recommendation:** interned children already carry
   their own `content_hash` (the `Access::Handle` arm hashes the child's cached hash,
   per `2-hashing-flesh.md`), so `Field`/`MapGet` should read the child's stored
   identity, not re-canonicalize. `Tag`/`TreePath`/`DocGet` over scalars stay cheap
   (inline). Open question: whether `DocGet`/`TreePath` on large sub-trees warrant a
   *per-projection* memo (identity-of-a-projection as a first-class cached fact) or
   whether the child-hash read suffices. Recommend measuring after trunks 1–2 land —
   the 13.6% may already be gone.
