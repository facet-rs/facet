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

## Round 3 (2026-07-08, post-review with Amos)

- **Vix 101 banked as law: `machine.scheduler.no-in-program-forcing`.**
  Everything is lazy, everything is demand-driven; construction binds
  promises; no in-program construct forces; forcing originates only outside
  the program (the vx CLI / the holder). `task-is-path` reworded — its
  "nested demand is a call" phrasing read as eager evaluation order; it now
  explicitly describes what happens when propagated demand REACHES an
  invocation. The spec asking "is vix lazy in constructors?" was a process
  failure: the language's born principle was never in question.
- **Rule syntax fixed corpus-wide.** All 164 rules (machine + solver) were
  written as bare `r[id]` + paragraphs; dodeca's binding requires the rule
  id AND its prose in ONE blockquote. Every rule is now its own `>` block;
  verified via `ddc coverage rule` showing definition bodies where it
  previously reported "No definition body" (the missed tell).
- **Research artifacts moved out of /tmp** (they die on reboot; one reboot
  already cost a rerun today) into ~/vixenware/notes/machine-spec/: all
  mining, poke, feel reports + the hash-as-field proposal branch copy +
  hostcall census + language-gap censuses.
- OPEN (with Fable to think through, per Amos): container identity over
  pending children — permanent promise-ids vs identity-migration on
  realization — narrowed by the laziness law (there is no eager
  construction to diverge from) but the realization story still needs
  design; the lazy-fields/Point example is the canonical test case.

## Round 4 (2026-07-08 afternoon — the islands conversation, Amos + Fable direct)

Foundation chapter written: docs/content/vix/_index.md ("What vix is").
Decisions banked in it, superseding/reframing rules pending the
reconciliation pass:

- **Islands are the missing IR** between AST and weavy IR: vixc partitions
  grains (computation sites of values) into islands (eager straight-line
  interiors) and edges (identity/memo/receipts/suspension/safepoints).
  Everything re-derives from the partition. NOT a separate spec — the
  foundation of this one.
- **Namespaces: `vix.*` (semantics) / `vixc.*` (compiler).** "machine" dies
  as a name — there is no machine, only an implementation. Charter rules
  (HostEnv, RefCell bans, ABI) split out of semantics during the rename
  sweep. Rename is near-free NOW (zero machine.* impl annotations exist).
- **Vix 101 sharpened: description, not action.** Programs describe value
  graphs; nothing in-program evaluates ANYTHING (no escape hatch — stricter
  than all prior lazy languages); demand exists only outside (vx CLI, LSP,
  audits). "Promise" vocabulary banned (JS eager-association + the noun was
  the bug: no wrapper objects, just wiring). Values, not nodes: everything
  is a value; dependencies full or partial (projections).
- **The as-if law** named as the master law: implementation may do anything
  unobservable at the semantic plane. Instances: molten mutation
  (uniqueness), rematerialization (sharing is economics, NOT a mandatory
  edge — softened from the earlier draft), eager interiors, suspension-as-
  discardable-replay-cache. By-value semantics = the wall that makes
  threads/executors/machines coordination-free (old vixen language
  doctrine, now written).
- **Partition-as-filter (Amos: "sold").** Memo keys = semantic value
  identity, partition-independent; the partition filters WHICH values are
  observed, never their keys. Deopt (split/merge islands at runtime) fully
  as-if; receipts survive repartition; compiler upgrades cannot poison
  cache; versioning surfaces reduce to identity-encoding epoch + lowering
  artifacts. Lowering determinism = reproducibility goal, not soundness.
- **No programmer draws islands** — cgu-style: maybe a how-much knob,
  never which/how; suboptimal partition = compiler bug; observable, never
  steerable. Door permanently shut.
- **Two-plane identity skeleton (to formalize as rules next): recipe
  identity (tier 1) vs value identity (tier 2)**, memo = the map between;
  downstream keys on value identities → early cutoff falls out (stdlib fn
  changes, value doesn't → one-node recompute). Same shape for pure fns,
  exec, solver facts. This ALSO resolves the pending-children identity
  think-item: grain/recipe identity exists at description time; value
  identity at computation; the old Pending-shares-identity smearing was
  the two planes collapsed.
- **Streams**: internal = codata (head, rest) — SICP lineage; stateful
  backing = molten under uniqueness; external streams = journaled effects.
  No new semantics.
- Dogfooding = design accelerator, not credibility requirement (rigor +
  oracles carry credibility). "Vixling" was dictation mangling of "old
  vixen language".
- PENDING: the reconciliation pass (all 152 rules: survive / re-derive /
  strike against the chapter), the vix/vixc rename sweep + charter split,
  molten-across-edges ruling (lean: never — merge islands instead; linear-
  handoff exception reserved for Stage B), formalizing tier-1/tier-2 rules.

## Round 5 (2026-07-08 evening — the location-plane conversation, Amos + Fable direct)

- **Tasks vs islands RATIFIED** (Amos: "we agree on tasks"): a task is a maximal
  inline path through the island graph — it traverses one or more islands,
  flowing through edges whose values are ready and parking/joining/splitting
  only at edges, never mid-interior. Not every edge is a task boundary. Kills
  are a separate category: they land at safepoints (edges + loop back-edges)
  and discard rather than suspend. Tokio analogy banked for the textbook:
  interiors are the synchronous stretches, edges are the await points, tasks
  are what the runtime schedules. `task-is-path` re-derives onto this.
- **Namespace for daemon/capability packages RATIFIED: `vixd.*`** (the daemon —
  completes vix/vixc/vixd; names the component, not the product). Physical
  home: ALL specifications consolidate into the facet monorepo (Amos: specs
  need scrutiny, iteration, public play; the proprietary boundary is the
  control plane / cloud parts, not the specs). The repo-topology problem
  (facet-cc too big to operate in, too small for a full build — no dodeca, no
  vixenware parts) is real and explicitly NOT solved by this ruling.
- **The location plane** (third identity plane: location/recipe/content) is
  designed and grounded — chapter: `docs/content/vix/three-planes.md`;
  prior-art grounding: `~/vixenware/notes/machine-spec/location-plane-prior-art.md`.
  Fills the `machine.persistence.trait-boundary` "enumerate projection
  candidates" stub. Rules to be extracted in the rewrite pass.
- **Ordering doctrine** (in `design/iteration.md`): positional order dies for
  derived aggregates (canonical value order; concurrency wins); construction
  stays positional (an array is a struct with fields named 0, 1, 2);
  `Indexed<T>` opt-in.
- **Spec-as-textbook doctrine** (Amos): the spec is a textbook for the
  amalgamation, not a list of prose-tests — chapters are written to teach the
  rationale, not just to bind rules.

### Round 5 addenda (same conversation, after the hash census)

- **Canonical order is CONTENT order, never hash order** (Amos: "content order
  not index order" — agreed landmine). The census found map ordering, dedup
  walks, and the value-ordering fallback comparing hash BYTES; that makes
  observable program output depend on the hash mechanism, turning any identity
  epoch break into a semantic change. Canonical ordering must be defined over
  value order (`<=>`); hash bytes may never be ordering-visible. Tripwire test
  owed.
- **ONE hash mechanism — the hybrid is an unblessed fork, not a design**
  (Amos, verbatim intent: "absolutely not... there is one mechanism, it has
  three different tiers"). The census's finding that the code runs raw-bytes
  hashing for flat descriptors and a framed walk for handle-bearing ones
  (driver.rs:1598) is a fork to ELIMINATE via the one sanctioned epoch, not a
  state to bless. Corollary: exec's private `exec::Blake3Hash` domain over
  plans violates `machine.primitive.requests-are-values` (requests are
  ordinary content-addressed vix values) — it joins the ad-hoc-cache kill
  list; exec keys through the same mechanism's tiers like everything else.
