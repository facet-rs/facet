+++
title = "machine: spec changelog"
+++

Amendment record. Every adversarial-review finding is listed with its
disposition: ACCEPTED (with how), DEFERRED (to an Amos decision, tracked as
an OPEN rule), or REJECTED (with reason). Four review seats, read-only:
correctness (codex-xhigh), consistency (codex-xhigh), completeness (opus),
testability (sonnet). Reports in `/tmp/spec-poke-*` and
`/tmp/machine-spec-poke-consistency.md`.

## Round 1 amendments (2026-07-08)

### Identity — the sharpest convergent findings

- **Value identity is `(SchemaRef, ContentHash)`, not `ContentHash` alone.**
  ACCEPTED (correctness #2, both seats). `blake3(memory)` collides
  `Bool(false)`/`Int(0)`/zero-newtypes/`None`. New rule
  `machine.identity.value-identity-pair`; `ContentHash` demoted to "the byte
  component." Memo keys, receipts, persistence claims all use the pair.

- **Pending and realized values do NOT share a content hash.** ACCEPTED
  (correctness #1, consistency M6). Flat-memory hashing makes it impossible.
  A pending invocation is keyed by its `DemandKey` (a `PromiseId`); the
  waiter matches on that, not on a shared `ContentHash`. The "recognize the
  value I awaited" property is served by the memo (`DemandKey → result`), not
  by identity collision. `machine.identity.pending-identity` rewritten;
  `machine.store.dedup` no longer claims shared-hash pending/realized slots.

- **Hash streams are framed (prefix-free).** ACCEPTED (correctness #10). The
  identity module exposes only framed writer APIs (domain/arity/field/variant/
  seq-len/map-pair/bytes-len); raw `hasher.update(user_bytes)` outside them is
  banned. New rule `machine.identity.framed-encoding`. This is what actually
  closes the ambiguous-concatenation surface the streaming-combine ban only
  half-closed.

- **`carried-hasher` scoped to ordered append-only aggregates.** ACCEPTED
  (correctness #9, consistency M3). Maps are sort-first-then-stream until the
  OPEN Merkle-map design lands; a carried streaming hasher over insertion
  order is unsound for maps.

- **Taint identity is a canonical leaf-set.** ACCEPTED (correctness #11).
  Union flattens (associative, commutative, idempotent) over sorted leaf
  taint ids; grouping cannot affect identity. `machine.identity.taint-in-identity`
  amended.

- **Molten values have no public final identity until freeze.** ACCEPTED
  (consistency M3). The carried identity of a molten aggregate is valid only
  over final child identities; a demand key over a molten aggregate forces
  freeze first. `machine.identity.hash-at-construction` amended.

- Naming notes (D1/D2, completeness): "Wagner k-sum" and "hash-as-field
  proposal"/"write-once identity slots" are session/committee vocabulary, not
  verbatim KB terms — noted in-rule so Law 30 (earned vocabulary) isn't
  self-violated. The mechanisms are faithful; the labels are ours.

### Scheduler — progress and safety

- **Admission bounds WIDTH, not depth; parking transfers the slot.** ACCEPTED
  (correctness #3, consistency M2, completeness F2/F3). The strict
  active+parked budget deadlocks on an acyclic chain longer than the budget —
  a real error in the battle-plan model. Fix: a task parking on a fresh child
  hands its admission slot to that child, so the deepest unfinished chain
  always progresses; the budget bounds concurrent independent paths.
  `machine.scheduler.live-budget` rewritten; new
  `machine.scheduler.progress-invariant`.

- **`join` is one atomic operation; wait-for cycles are typed errors.**
  ACCEPTED (correctness #4, consistency implied). `join(key, waiter)` under a
  single scheduler mutation either returns the memoized result or installs the
  waiter before any `publish` can drain it (no lost wakeup). A wait-for cycle
  is a `MachineError` unless the key class declares fixpoint semantics. New
  `machine.scheduler.join-atomic`.

### Effect lifecycle — the biggest coverage gap (new page)

- **New page `machine/lifecycle.md`.** ACCEPTED (consistency B4/B5,
  completeness F1). Covers: effect failure as a receipted result vs
  `MachineError`; ticket liveness (deadline/lease/cancel — no clock in the
  scheduler, but a cancellation primitive); freeze/publish atomicity
  (transactional at the root; partial children are unreachable garbage);
  poison ordering (poison is part of the atomic publish decision; the watch
  window extends through publish; post-publish poison revokes); world
  snapshots (every demand/effect runs against a stable snapshot — closes the
  last-write-wins hole, correctness #6).

### Receipts & memo — ownership and soundness

- **Verification reads are not receipt-recorded.** ACCEPTED (consistency B3).
  Reads performed by the machine to re-verify a projection candidate are
  machine-meta operations, not the demanded computation's reads. Stated in
  `machine.memo.verified-reuse`.

- **Memo hits remap the cached read-set into the caller's receipt.** ACCEPTED
  (consistency B3, preserved driver behavior). A nested hit contributes the
  cached entry's read-set to the caller's; exact hits stay allocation-free by
  pre-materializing exposure on miss. New `machine.memo.receipt-remap`.

- **Exact memo lookup compares the key preimage after the digest hit.**
  ACCEPTED (correctness #7). `DemandKey` collision must not serve a wrong
  value; entries carry `(closure identity, arity, [(SchemaRef, ContentHash)])`
  and compare it. Persistent untrusted exact claims are rejected unless policy
  permits the collision-resistance assumption. `machine.memo.demand-key`
  amended.

- **Receipt-of-receipt is exempt.** ACCEPTED (consistency M8). Demanding a
  read-set certificate is a machine-meta demand that does not itself produce a
  second-order receipt. `machine.receipt.exposed-to-programs` amended.

- **Primitive confinement contract.** ACCEPTED (correctness #5). `Hermetic`
  requires determinism plus interposition for all non-store inputs; a
  host-trusting backend (ambient OS/global reads it cannot witness) forces
  `Volatile` or non-persistent claims. `EffectCtx` witness discipline alone is
  not the hermeticity proof. `machine.primitive.memo-policy` amended;
  `machine.primitive.exec-hermetic-traps` cross-references.

- **Path/search resolution records candidate misses.** ACCEPTED (correctness
  #14, consistency). Search-path/PATH/include/symlink/enumeration decisions
  that affect the chosen path are recorded observations; the grammar declares
  which arguments trigger resolution. `machine.receipt.misses-recorded`
  amended.

### Caches, capability, persistence

- **Artifact-probe is a memo event family, not a fourth cache.** ACCEPTED
  (correctness #8, consistency M1). Four reuse *event families*, three cache
  *kinds*. `machine.arch.reuse-axes-distinct` and
  `machine.obs.event-vocabulary` amended; probes are memoized primitive calls.

- **Semantic comparators are demanded/memoized.** ACCEPTED (consistency B1).
  The "direct weavy call, not a full demand" language conflated the
  architecture (comparators ARE the semantic memo tier) with a perf concern
  (the comparator body must be efficient — native ops, no per-pair
  allocation). `machine.execution.comparator-direct` rewritten to state the
  perf property without denying the demand.

- **Single capability-fingerprint authority.** ACCEPTED (correctness #13,
  consistency M5). The daemon advertises the fingerprint (source of truth);
  the backend probe VERIFIES it or emits poison, never silently mints a new
  identity. `machine.capability.fingerprint-in-identity` and
  `machine.primitive.exec-probed-toolchain` reconciled.

- **Only realized-tier values persist; the persistence key needs no tier.**
  ACCEPTED (consistency B2). `machine.persistence.ephemeral-stays-ephemeral`
  already bars pending/scheduler state; stating that persisted values are all
  realized tier makes `(SchemaRef, ContentHash)` sufficient at the persistence
  boundary without contradicting the store's tier axis.

- **Persistent exact claims re-verify.** ACCEPTED (correctness #12). Exact
  claims are read-set-gated too, unless proven pure over content-addressed
  arguments only. `machine.persistence.lookup-order` amended.

- **`machine.persistence.trait-boundary` downgraded SETTLED → DESIGN.**
  ACCEPTED (completeness M1). The interface is LOAD-BEARING but its shape
  comes from a doc with open questions; it is not decreed.

- **Journal is a distinct mandatory observation store.** ACCEPTED (consistency
  B6/m2). Separated from the no-op-able event sink and from the banned "fetch
  journal cache" (a naming collision). New `machine.receipt.journal`.

### Restored dropped rules (completeness C1–C5)

- `machine.identity.hashing-is-ambient` (C1) — content-hashing is a free,
  always-available property of any DAG value (warm-facts `proof_digest`
  depends on it).
- `machine.scheduler.demand-services` (C2) — the demand/call Class-C surface
  (invoke, pending alloc/coerce/invoke, tree project/text, array-map-pending).
- `machine.scheduler.observation-recording` (C3) — acquire journals dedupe by
  hash with a timestamped event.
- `machine.capability.projectability-owned` (C4) — projectability owned by
  capabilities over SchemaRef.
- `machine.obs.snapshot-no-clone` (C5) — L11's observability half, restored.

## Round 2 — Amos decisions (2026-07-08)

- **phon-jit scope RESOLVED → `machine.execution.jit-single-feature`.** One jit
  feature in the ecosystem: weavy's, a master switch (`jit_active = feature_on
  ∧ platform_supports`). phon and vix carry none. Off = off for good; on = on
  where the platform allows executable memory. The forwarding-feature wrench is
  removed via `links = "weavy"` + `DEP_WEAVY_JIT` (a dependent's build script
  follows weavy's single decision without owning a feature) and a build-script
  platform gate that force-disables on W^X-locked targets. API surface always
  compiled; only the copy-patch runtime executor + stencil extraction are
  behind `weavy_jit_active`. Key correction that made it clean: compiling the
  copy-patch machinery is build-time waste, not runtime W+X — so the feature is
  about waste and single-source-of-truth, not a hard iOS blocker.

- **Executions-as-weavy-tasks RESOLVED → the replay/suspension cluster.**
  Amos's synthesis dissolved the suspend-vs-restart fork by making them
  different kinds of thing: RESTART IS THE SEMANTICS
  (`scheduler.replay-is-semantics` — kill-anytime is always sound; canonical
  execution state is memo + demand map), SUSPENSION IS THE ACCELERATION
  (`scheduler.suspension-is-acceleration` — executions run as weavy tasks;
  parked state is a discardable replay cache). The executor's interp/JIT
  relationship, applied to the scheduler; the store's molten/interned
  duality, applied to scheduling state. Supporting rules:
  `tickets-outlive-tasks` (effects owned by demands),
  `eviction-is-policy` (parked memory is an evictable cache; migration =
  kill + ship DemandKey + replay), `chaos-kill-oracle` (SETTLED day one:
  standing CI chaos mode randomly kills tasks and asserts identical
  results), and `execution.safepoints` (lowering-injected patchable
  safepoints, multiplexed for kill barriers / perf counters / future GC —
  lowering acknowledged as load-bearing substrate for the whole monorepo).
  Two named taxes accepted: the equivalence discipline is forever
  (chaos oracle enforces), and safepoint placement is real, perf-gated
  lowering work. Side effect: `receipt.certificate-vs-derivation` is now
  easier — replay makes a derivation re-obtainable on demand, so "walkable"
  need not mean "retained."

## Deferred to Amos (remaining OPEN rules)

- `machine.value.taint-provenance` — V30 vs secrets Q1 taint granularity.
- `machine.receipt.certificate-vs-derivation` — walkable derivation
  (note: weakened by the replay resolution; see above).
- `machine.receipt.sealable-as-cachet` sub-question — cachet vs secret sealing.

## Deferred to a later pass (testability, not correctness)

- Splitting crammed rules (`obs.event-vocabulary`, `receipt.granularity`,
  `store.construction-services`, `obs.counters`) into granular sub-rules so
  one `r[impl]` tag cannot show false-positive full coverage.
- Renaming topic-named OPEN rules to claim-named, adopting the
  `[SETTLED, scope OPEN]` split pattern for each.
- Confirming the R1–R4 minimal `r[impl]` sets against the battle plan (the
  rung boundaries were inferred, not grounded).

## Rejected

- None outright. The page-count nit (correctness #15) is a report-vs-prompt
  mismatch, not a spec defect: there are 15 pages by design, and the index
  now says so.
