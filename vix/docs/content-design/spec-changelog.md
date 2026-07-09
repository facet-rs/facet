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

### Round 5, final addendum — the content-hash ruling (Amos)

- **Content hash = Definition A: the framed walked encoding**, schema-
  specialized and LOWERED AS WEAVY IR (schema-const walk, framing constants
  folded; the descriptor-interpreting Rust implementation was an
  implementation crime, not the definition's nature). Deciding principle,
  stated once and forever: **the structural hash of a value must not depend
  on the ABI** — layout exists to be changed for performance (NRVO, niches,
  reordering), and coupling identity to it would freeze the very freedom
  weavy-declared layout exists to provide. B was a plane smear (implementation
  plane leaking into the semantic plane) — the same disease as
  Pending-shares-identity, killed the same way.
- Consequences for the rewrite: `identity.canonical-memory` STRIKES as an
  identity mechanism (zero-padding demotes from identity-load-bearing to
  hygiene/canary — its rule gets re-scoped); the driver's flat-identity raw
  fast path (driver.rs:1598) is the unblessed fork's dead branch; lowering
  the walk into weavy IR becomes a design requirement of the successor
  (`execution.facts-precomputed` applies — the specialized walk is a lowering
  artifact).
- Context recorded for the perf lane: the walk was twice measured at "0.03%
  of execution time, not worth lowering" — against a program later shown to
  be ~70,000× slower than reference. A number without its control is a vibe
  (resolution 3); hashing cost must be re-measured against the successor's
  baseline, never the old engine's.
- Part-1 confirmation recorded without ceremony: the bespoke warm-facts
  subsystem is dead (the design conversation had already collapsed it; the
  thorough check found zero soundness residue). Its inverted index survives
  only as the nomination design's worked precedent.

- **Entry-carried identity RATIFIED, "hash-as-field" name retired** (Amos: "I
  would not call it a field, but yeah, sure"). The mechanism: the content hash
  is a property of the STORE ENTRY (beside bytes/refcount — never a slot in
  any value's layout; inline scalars exempt), written once at intern (walk
  reads children's stored hashes, O(own bytes + 32×handles), once per node
  ever), read as a load at every later edge; plus the molten carried midstate
  in MoltenEntry bookkeeping as a droppable cache (freeze = O(1) finalize when
  valid, full specialized walk when dropped; differential tripwire: same
  digest both ways). The proposal's §4 flat-memory fallback is amputated with
  definition B. Rewrite vocabulary: entry-carried identity + carried midstate,
  not "hash-as-field".

### Round 5 — surface rulings (Amos, continued)

- **Mutation audit ORDERED**: the current documentation likely assumes
  mutation in places because the syntax reads like Rust — audit every doc
  page for semantic-plane mutation claims vs the by-value law (sanctioned
  as-if/molten implementation talk is fine when labeled as such).
- **Names carry semantics**: no bare `map`/`pop` whose ordering/selection
  semantics are implicit — variants get longer, explicit names. Execution
  shape (fused vs fan-out) stays OUT of names (partition's business);
  only semantic differences (positional vs unordered, min vs max) may
  appear in a name.
- **pop is by-value or nothing**: any "removal" returns (element, rest) as
  fresh values; which named variants exist (last / min / max / first-ready?)
  is open — first-ready is in tension with the determinism doctrine, to be
  argued in the draft.
- **Indexed<T> RATIFIED**: plain std alias `(Int, T)` + enumerate. Nothing
  special.
- **map's return type: still open** — stream was tried and rejected in the
  original design ("not conducive"); a mutable bag violates by-value; the
  named-variants approach reframes the question (positional field-wise map
  on arrays is honest struct-mapping; the unordered result type must be an
  immutable multiset VALUE, born complete, name TBD).
- **Builtin-vs-std dividing line TBD**: combinators expressible in vix
  should be vix; some will be builtins; line undecided.
- **Combinator surface to be DRAFTED as documentation** for website review,
  not enumerated in conversation. For-loops and the trajectory-class rule
  (B2): circle back later, explicitly.

## Round 6 (2026-07-09 — the syntax session, Amos + Fable direct)

- **RATIFIED, one word each**: unary minus; array spread `[..a, ..b]`;
  string interpolation via backtick templates with `${}` (plain `""` stays
  literal — distance-honest); map literals `%{ k => v }` (the ratchet's
  casual `{}` is dead; swept). Rungs and book updated in the same breath.
- **Methods: `namespace Type { }` + import-scoped `extend Type` accepted
  as the direction** — it also gives `<=>` implementations a home and
  answers coherence/orphans Rust-style without impl-block baggage.
- **Test surface: converging** — a test is an ordinary `fn(t: Test) ->
  Test` under `#[test]` (attributes owed anyway for decode annotations);
  trace expectations are ordinary calls BECAUSE arguments are wires
  (never_demanded(expr) holds the wire, never demands it). The expecting
  mini-grammar and header directives are dead. OPEN: the accumulator
  ergonomics (see rebind question).
- **OPEN, scheduled**: block delimiters (Amos has a parens-blocks model —
  cascades into whether `{}` could be data-only, which would reopen the
  map-literal spelling); path syntax ergonomics (zoo's four `@` forms vs
  `p""` + `/` — build languages construct paths constantly; missing from
  the table until Amos flagged it); positional application & `partial`
  (partial is wanted; hinges on the application-style comfort);
  rebind/update sugar vs blessed chaining for by-value accumulators.

### Round 6 addenda

- **Generators have NO special signature syntax** (Amos: "concurrency is
  magic — you only need magic syntax if you don't have magic concurrency").
  A generator is an ordinary function whose return type is `Stream<T>`;
  `yield` appears in the body; the type is the whole declaration.
  `#[test] fn name() -> Stream<Check>` is the test shape. Streams are
  codata, confirmed. `yields` keyword: dead on arrival.
- **must_use ADOPTED** as a type marker (Check/Test-class types): a bound
  value of a must-use type that is never used is a compile error — even
  under generators you can construct a check and forget to yield it.
- **`.=` rebind sugar: OPEN** ("I don't know") — parked in the ledger, not
  banked, not dead.
- **THE PORT CAMPAIGN (Amos directive, verbatim intent)**: before ANY
  implementation, port the entire essential corpus — rodin, index,
  cargo_manifest, crate — to the language as designed; look at all of it;
  agree on semantics from the feel of real code; then unleash analysis
  bots on book+corpus hunting underspecification, wishy-washiness, and
  implementation hazards. "I want to be serious about this now."

### Round 7 — ordering (Amos ruling)

- **`<=>` subsumes the comparison family.** Saying a value supports "`==` and
  `<=>`" was redundant; `==`, `<`, `<=`, `>`, `>=` all derive from `<=>`. A type
  never defines them separately.
- ~~**`<=>` is user-overridable, and when overridden it IS the type's order.**
  Answer (B).~~ **STRUCK in round 9, superseded by orders-as-values.** `<=>` is
  the STRUCTURAL comparison: derived from a type's fields in declaration order,
  total, and NOT overridable. Anything else is an `Order<T>` you pass. The
  ruling improved rather than reversed — see round 9. What survives verbatim:
  the terms "canonical order" and "structural order" are not two concepts; a
  value has one order, and ordered things are simply ordered.
- **`<=>` must be a total order.** A comparison answering `Equal` for values
  that are not equal is not a `<=>` — it is an ordinary function. The compiler
  cannot prove totality; it is an obligation, like `sorted_by`'s.
  - Consequence: `sorted_by`'s "ties are broken canonically" is struck. There
    is no second order underneath to break ties with.
  - Consequence: `rodin.vix:74`'s `namespace Version { fn <=> }` is not a valid
    `<=>` — it answers `Equal` for versions differing in prerelease/build.
- **`r[machine.identity.map-order-independence]` is NOT settled** (Amos: "it's
  settled my ass; I never agreed to any of that"). Insertion-order-independence
  stands. "A map is a set of pairs" is rejected. Sort-first-then-hash over key
  order is a hole: it defines content identity in terms of user-overridable
  `<=>`. New rule `r[machine.identity.never-consults-order]` states the
  constraint (identity is a function of content alone); the mechanism is OPEN,
  and no rule exists for `Multiset`/`Set` identity at all.
- Custom operators beyond `<=>`: Amos is open to a **small fixed set** (versions
  are load-bearing enough to earn one) but operator syntax is never
  user-extensible — parsing must not be customizable.

### Round 8 — calling convention, records, type identity (Amos rulings)

**Vocabulary struck.** "Wire" is rejected for an undemanded value: it collides
with `vix-wire` (the executor over the real vox wire) and, worse, it reinvents
the noun that `promise` was banned for. There is no object. There are values;
values are not computed until something demands them. The book already says it
without a noun (`_index.md:40`: "it denotes a value that depends on `getx()`'s
value"). `dependency` survives as a *relation* ("depends on"), never as a noun
for a value.

**Calling convention.**
- Parentheses are grouping. They are never call syntax.
- Application is juxtaposition: a value followed by a value calls the first
  with the second. `f (x)` is `f x`.
- **At most one positional argument** — the subject, the thing acted on.
  Everything else is named. (Lineage: Swift, Smalltalk. The zoo's `.vx` files
  are inconsistent trials, not a prior design; do not read them as one.)
- `f x y` still parses (left-assoc) and type-errors unless `f x` is callable.
  At-most-one removes the ambiguity, not the grammar. Precedence, tightest
  first: `.field`/`.method` > juxtaposition > unary minus > binary ops >
  `where { }` (postfix on the application).
- **Unary minus + juxtaposition: defend by rejection.** `abs -1` would parse as
  subtraction. A juxtaposed argument must be an *atom* (identifier, literal,
  string, path, bracketed group); `-1` is not, so `abs (-1)` is required and the
  compiler says so with a fix-it. Whitespace-sensitive lexing (Swift's rule) was
  considered and rejected: invisible in a diff.

**Named arguments are a record.**
- `where` in a signature declares the named-argument record type; `where { }` at
  the call site constructs it. `where` sits LEFT of `->`, because it names
  inputs and inputs belong left of the arrow. (Rust's `where` is a different
  word that happens to be spelled the same.)
- Inline `where { mounts: [Mount] = [], ... }` declares a **structural**
  (anonymous) record type: this function's one-off option set.
  `where ExecOpts` names a **nominal** one: a value, storable, decodable, and
  *spreadable* across a family of functions. Not two styles — one-off vs family.
- `name: value` everywhere. Never `name = value`. Therefore attributes take
  records: `#[test { budget_wall: 5s, budget_rss: 1GB }]`.
- **Suffix literals**: `5s` is a `Duration`, `1GB` a `ByteSize`. The suffix set
  is CLOSED and language-defined; users never add suffixes (same line as
  operators: no user-extensible parsing). This kills the `//! budget: "5s"`
  naked-String-where-a-newtype-belongs (V6 disease).
- **Punning**: bare names inside braces (`where { mounts, observer }`,
  `Guard { parent, dep, req }`). NOT `:foo` — that spelling is reserved: the zoo
  used `:cc`/`:ar`/`:ranlib`/`:orb` symbols and `Map[Symbol, DiscoveredTool]`,
  and symbols are on the salvage list. 115 stutter sites in the ported corpus
  are waiting on this; it is owed independently of `where`.
- Defaults alone dissolve the corpus's five `X` / `X_target` twin pairs.

**Two kinds of type.** Not `struct` vs `record` as spellings — nominal vs
structural *identity*, and taxon already implements the answer:
`taxon/src/identity.rs:242` (`r[impl schema-identity.canonical-encoding]`)
writes the type NAME into the canonical encoding.
- **Nominal**: identity is name + shape. `Point{x,y}` != `Vec2{x,y}`. This is
  what makes NEWTYPES work — without it `struct Meters(Int)` and
  `struct Seconds(Int)` hash identically and a newtype protects nothing.
- **Structural**: `record { ... }` — identity is shape alone, no name to hash.
  Two identical anonymous records in different modules are the same type.
  `Point { x: 1, y: 2 }` and `record { x: 1, y: 2 }` have different SchemaIds,
  hence different content hashes: never equal, never a memo collision.
- The `record` keyword also disambiguates juxtaposition: once `f x` is a call,
  `f { ... }` must be decidable. `f record { a: 1 }` is unambiguous, `where { }`
  is marked by `where`, and bare `{ ... }` stays free for `Set` (no chapter yet
  — free now, expensive later).
- SPEC NAMING BUG: "canonical *structural* encoding" reads as structural typing
  and means "an encoding obtained by walking the structure" — which embeds names
  and yields NOMINAL identity. Say so in the rule, in one sentence.

**Spread.** You may always drop a name; you must always earn one.
- `record { ..point }` — name erasure. A projection: total, always legal.
- `Vec2 { ..point }` where `point: Point` — REJECTED. Fix-it:
  `Vec2 { ..record { ..point } }`. Laundering by destructure-and-rebuild was
  always possible; the rule buys that conversion is never SILENT. The rejection
  is the teaching — this wants a `.reject.vix` rung.
- `Vec2 { ..r }` where `r` is a record — legal if `r` supplies exactly the
  fields `Vec2` needs (with the literal's explicit fields filling the rest).
  Extra fields in `r` are an ERROR: silently dropping data is the bug spread is
  otherwise perfect for hiding. [Amos: rule the extra-fields clause.]
- `Point { ..point, x: 3 }` — same-type spread, as Rust.

**The 257th symbol.** Suffix sorting appends `$` because `$` cannot occur in the
text; the extra slot is not the point, the disjointness is. Today the kind tag
is a length-prefixed *string* drawn from the same alphabet as type names
(`write_str(out, "struct")`, `sink.rs:36`) — injective by the walker's
POSITIONAL discipline, not by construction. Emit the kind as a byte discriminant
from a closed set (struct / enum / record / primitive) that no name can produce.
Then injectivity is structural, and the anonymous-record tag needs no empty-name
hack. COST: rehashes every SchemaId. Ride the pending **stage-6 identity
freeze** (NEXT.md:94) rather than minting an epoch — and note `.vix-cas` now
exists on disk (real_process.rs:322), so post-freeze breaks are not free.

**Still open / not ruled:** blocks-as-parens (implied by parens-being-grouping,
never ruled); the extra-fields clause above; whether the `struct` declaration
keyword should change now that `record` names the other kind; path syntax;
`partial` (its natural home is `f x y`, which now parses).

**Unrelated landmine, flagged not fixed:** `legacy_marker_schema_id` uses std
`DefaultHasher` (unstable across Rust versions) and REACHES CONTENT HASHES
(NEXT.md epoch-closing flags). A content hash that moves when rustc moves is a
verification bug, not a cache-miss bug, and `r[machine.identity.hasher-contract]`
already forbids it. Must die in stage-6.

### Round 8 addenda (Amos)

- **Extra fields on a record→nominal spread are an ERROR.** RULED. Silently
  dropping a field is the bug spread is otherwise perfect for hiding.
- **Braces stay blocks; parens-as-blocks is DEAD.** The only argument for it was
  that `{ }` was overloaded (blocks / records / grouping). Grouping is parens —
  it is just an expression. Records are keyword- or type-prefixed. So braces are
  free, and the parens-block model dies of its own success.
  - CORRECTION to round 8: bare `{ ... }` does NOT "stay free for `Set`". It is
    a block. `Set` needs its own marker when it gets a chapter (queue item A2).
- **ONE keyword: `struct`.** `record` as a keyword is dropped; it survives as the
  concept noun, as in Rust. Don't spend a keyword to say "unnamed" — the absence
  of the name already says it, and the name's presence is exactly what *causes*
  the nominal/structural split in the canonical encoding.
  - `struct Point { x: Int }` — declaration, nominal, name is hashed.
  - `Point { x: 1 }` — nominal literal, type-prefixed.
  - `struct { x: 1 }` — anonymous literal, structural, no name to hash.
  - Still disambiguates juxtaposition: `f struct { a: 1 }` vs `f { let x = 1; x }`.
- **`partial` is DEAD, and now for a reason rather than by deferral.** `f x y`
  parses as `(f x) y`, which is an ordinary higher-order return, not currying —
  at-most-one-positional leaves no slot to curry into. The zoo's `partial` meant
  "pre-bind arguments without restating the call surface"; since named arguments
  ARE a record, pre-binding them is record construction (`let opts = ExecOpts {..}`
  then `where { ..opts }`), and pre-binding the positional is a closure. Both
  covered, no keyword.
- **SchemaId rehash cost: zero.** No deployed users of the current identity bytes
  (Amos). The byte-discriminant kind tag rides stage-6 freely.

**Correction, on the record.** Round 8 relayed a NEXT.md flag claiming
`legacy_marker_schema_id` uses std `DefaultHasher` and reaches content hashes.
That is FALSE — `vix/src/module.rs:1071` uses blake3, domain-separated and
length-prefixed; there is no `DefaultHasher` in `vix/src` at all. The flag was
stale and was relayed without reading the code (the exact failure the standing
instruction forbids). NEXT.md is corrected in place. What survives, and is the
real stage-6 item: `legacy_marker_schema_id` derives a SchemaId from the type's
*rendered name string* rather than from its structure — identity by spelling.

### Round 9 — orders are values; streams are the default; multisets die

**RULED (Amos, explicit).**

- **`<=>` is the structural comparison.** Derived from a type's fields in
  declaration order. Total. NOT overridable — there is no `namespace T { fn <=> }`.
  Supersedes round 7's answer (B), struck in place above.
- **Anything else is an `Order<T>` you pass.** Orders are ordinary values.
  `by_key(f)` ranks by the structural order of `f(x)`, ties broken by the
  structural order of `x` — total BY CONSTRUCTION, and consistent with `==` for
  free. The unprovable-totality obligation cannot be stated any more.
  Consequence: `rodin.vix`'s `namespace Version { fn <=> }` is deleted, and
  `Version` instead declares `pre: PreTag` where `enum PreTag { Prerelease(..),
  Release }` — the variant order IS semver's rule. **If a type's structural
  order is wrong, the type is wrong.**
- **`#[key]` on structs: dropped** (Amos: "forget #[key] it was silly"). Intrinsic
  order has no knob. The pattern, stated once: *intrinsic properties come from
  declarations, extrinsic ones from arguments.* (Same shape as nominal-vs-
  structural type identity, and as intrinsic order vs `Order<T>`.)
- **Multisets die.** "A set that allows multiples because it isn't hashed is an
  array. It's like making a not-hashed hashmap." Every stored `Multiset` in the
  ported corpus (rodin: `features`, `learned`, `packages`) is a `Set` with a
  hand-maintained uniqueness invariant — the porter says so at `rodin.vix:15`.
  Zero genuine bags in 5,476 lines.
- **Arrays are not the default reflex.** Most things are streams. An array is
  either authored (order is data you wrote — library link order, include search
  paths) or the result of `collect` (order is a rule you named). Positions have
  exactly two provenances: **you wrote them, or you sorted them.** Never the
  filesystem's, never the scheduler's.
- **`glob` returns a stream, not an array.** `cargo_manifest.vix:292` returns
  `[String]` straight out of `workspace.glob(...)` with no sort: the positions
  come from `readdir`, flow into argv, and make the link non-reproducible. This
  is the mundane instance of the whole arrival-order problem — it is in every
  build anyone will write, and it has nothing to do with observers.
- **A stream is NOT ordered.** Its order is arrival order — a scheduling
  artifact, not a property. Two-plane restatement: a stream has RECIPE identity
  and no VALUE identity. Its elements are ordinary demands (cached individually);
  the aggregate has no content hash until resolved.
- **Generators do not yield in yield order.** `yield a; yield b;` may deliver `b`
  first — what comes out is availability order. Highest-ranked entry in the
  innovation-points ledger: a construct whose entire mental model in every other
  language is "resumes in order." `testing.md` is written as though yield order
  were real. It also makes the phase-partition of `Check` (value checks demanded
  during the run, trace checks after) NECESSARY, not tidy — there is no "last
  yield" to put `never_demanded` in.

**UNDER DISCUSSION (Amos: "still thinking about this") — not banked.**

- **`Keyed<K, T>`**: stream elements carry their provenance. `[3,2,1]` streams as
  `Keyed(0,3), Keyed(1,2), Keyed(2,1)`; keys flow through `map`/`filter`; compose
  under `flat_map` into a path; `.unkey()` strips. Retires `enumerate`,
  `Indexed<T>`, and — crucially — **filter's compaction problem**, which was the
  entire argument for multisets existing (`collections.md:174`: "survivor #2 sits
  at index 1, which silently forces an order"). Keep the key, nothing renumbers.
  Recognised mid-conversation as the LOCATION PLANE surfacing into the type
  system ("input-position keying for fan-out; locations flow through derivations
  = provenance", `three-planes.md`).
  - If `struct Keyed<K,T> { key: K, value: T }`, then structural order sorts by
    key already, so `collect()` needs no `order:` parameter and sorting an
    artifact by its content is unreachable. The earlier `collect(self) where {
    order: Order<T> }  // no default` proposal is superseded by the declaration.
  - OPEN: implicit `Keyed<K,T> -> T` coercion (recommendation: NO — key erasure
    should be a confession, like `record { ..point }`); whether `collect()`
    returns `[Keyed<K,T>]` or `[T]`.
- **Amos's "taint"** = a value whose recipe is deterministic but whose content is
  not, therefore un-memoizable ("the individual things are cached; the aggregate
  is not, until you resolve it"). **NAMING COLLISION**: `taint` in the spec today
  means *secrets* (`r[machine.primitive.sealed-boundary]`, seal/declassify;
  `r[machine.identity.taint-in-identity]` is its propagation algebra). Opposite
  consequence, too — a sealed value may be nondeterministic and the memo merely
  MISSES; an arrival-ordered value makes the memo LIE. Do not land Amos's concept
  under that word.
  - Two axes, not one: **deterministic** (result independent of arrival order)
    and **early** (may commit before drain). `min`/`count`/`collect` are
    deterministic and never early; `head`/`take(n)` are early and never
    deterministic. The both-boxes set is exactly the MONOTONE reads — `any`,
    `all`, `contains` — where an unarrived element cannot change the answer.
    Prior art: LVars' threshold reads (Kuper & Newton), CALM (Hellerstein);
    already banked in-house as "monotone propagation = confluent = race-safe".
  - **rmeta readiness is monotone** — once ready, always ready — so the observer's
    "has a line matching P arrived?" is a threshold read: early AND deterministic.
    That is *why* progressive trees are sound, not a special case.
  - Discharges: **canonicalize** (collect/sort) or **journal** (record the order,
    making it an input; replay reproduces). FLAG: there are two "journal"s in the
    docs pointing opposite ways — `machine/_index.md:40` names *"fetch journals"*
    as a COUNTER-EXAMPLE and `r[machine.cache.effects-are-memo-entries]` forbids
    private effect tables, while `scheduler.md:122` journals capability
    observations approvingly. Resolve before leaning on it.
  - Still open: does `fold` on a stream exist (commutative-associative is
    unprovable) or must you `collect()` first?

**RULED: `%` is not modulo.** There is no `%` operator in vix. Truncated and
Euclidean remainder are `.rem()` / `.rem_euclid()` if needed — the `-7 % 3` sign
convention was always an operator deciding silently, which the names-carry-
semantics ruling forbids. `%` is a prefix collection sigil only: `%{k => v}` map,
`%[a, b]` set. This was forced: `%` was infix modulo in six rungs, and
juxtaposition (round 8) made `f %[a, b]` and `f %{k => v}` ambiguous against it —
round 8 broke round 6 retroactively, and nobody would have noticed until the
first `exec cmd where { env: %{...} }`. Sweep: six rung files.

### Round 9 addenda — the map model, ratified with two corrections

- **RULED: `collect()` has exactly one return type, `Map<K,V>`.** No polymorphic
  return, no `FromIterator`, no turbofish. Rust's `.collect()` is cursed because
  inference picks the type; ours may not. Consequence: `Tree` must be an ACTUAL
  ALIAS over `Map<Path, Blob>` (not a newtype), or the exec case doesn't line up.
- **`[T]` cannot be an alias.** Its keys are `0..n-1` and density is an INVARIANT,
  not a shape; we have no refinement typing. So `[T]` is a real type, reached only
  through `.values()`. Three of the four are aliases (`Set<T>` = `Map<T,()>`,
  `Tree` = `Map<Path,Blob>`, `Map<K,V>` itself); the array is the odd one out
  because **an array's keys are not stored — they are the field names.** That is
  what makes it dense by construction, and why it is rare.
- **CORRECTION (Amos): progressive exec trees are NOT a `Stream<Path,Blob>` that
  collects into a `Tree`.** The VFS is not write-only: a file can be written,
  truncated, reopened, rewritten. So the event stream has DUPLICATE KEYS, and
  `collect()` — which fails on duplicates — is the wrong operation. The tree is a
  FOLD (last write wins per path), not a collect. This re-proves the
  exec-observers note from the other direction: readiness is protocol, not
  filesystem, because the filesystem event stream cannot say a file is finished.
- **`Keyed<K,V>` is deleted as a type.** The key is a parameter of the STREAM, not
  a wrapper on the element — otherwise `map(compile)` hands the closure a row to
  rebuild, which is `enumerate` with extra steps. `flat_map` is the tell: composing
  keys into a path is only statable if `K` is a stream parameter.
  ```
  Stream<K, V>
      map(f: V -> U)                 -> Stream<K, U>          key untouched
      filter(p: V -> Bool)           -> Stream<K, V>          nothing renumbers
      flat_map(f: V -> Stream<J, U>) -> Stream<(K, J), U>     keys compose
      collect()                      -> Map<K, V>
  Map<K,V>.stream()                  -> Stream<K, V>
  Map<K,V>.values()                  -> [V]                   the one compaction
  ```
- **`collect()` can only fail if you rekeyed.** `map` preserves keys, `filter`
  preserves keys, `flat_map` extends them — so a duplicate key is always
  attributable to exactly one `rekey` call. Free error attribution.
- **The unification is CONCEPTUAL, not identity-level.** Names are hashed into
  SchemaId (round 8), so `[a,b]` and `%{0=>a, 1=>b}` must not share a content
  hash. Four names, one structure, one implementation. A `Tree` can never
  accidentally equal a `Map`.
- **OPEN**: a set's stream wants `Stream<T,T>` (key and value both the element) so
  that `set.map(f)` means "the map from each element to its image" — which is
  exactly the gcc build. But `Set<T> = Map<T,()>` says `Stream<T,()>`. Lean:
  `Stream<T,T>`, stored once by the as-if law.
- **`ExecTree` is wrong today** (`exec-protocol/src/lib.rs:284`): two `BTreeMap`s
  over one key space (`entries: <Path,Text>`, `blobs: <Path,Vec<u8>>`), and
  `insert_bytes` chooses between them by whether the bytes are valid UTF-8. A path
  can be in both; `bytes()` prefers `entries`, so the blob becomes a ghost. And
  since it derives `Facet`, "is this file valid UTF-8" is part of the tree's
  identity STRUCTURE — an implementation convenience leaking into the schema.
  Should be `Map<Path, Blob>` with `text(p)` as a decoding projection.

### Round 10 — effects, capabilities, placement

**RULED (Amos, explicit).**

- **Streams cross island edges as codata.** The edge's semantic content is the
  value the stream drains to; the incremental view is as-if. This is what lets a
  process's interpreter be a *separate demand* — change the interpreter, don't
  rerun `rustc` — while still consuming progressively. Replay is the semantics;
  live consumption is the fast path.
  - ASYMMETRY OWED: molten values may not cross an island edge (lean "never —
    merge islands"). Molten and codata are structurally the same problem. If one
    crosses and the other does not, that must be principled. It is not yet.
- **Stop conflating islands with placement.** An island edge carries a *value*
  between two computations in ONE evaluator. A `place` carries a *subgraph of
  demands* to a DIFFERENT evaluator. `place` is a stronger boundary. Most of the
  difficulty in squaring the two came from treating placement as an island edge.
- **`exec` is a boring effect**, like `fetch`. Not an exception. Running a
  compiler is not a side effect — it is an expensive way to compute a value on a
  machine that has a compiler. Effects are already mandatory island cuts
  (`_index.md:135`), so **an island contains at most one effect, hence at most one
  capability requirement**: capability disjointness is a THEOREM, not a new rule.
  The `objcopy`-before-transfer case is therefore not a partitioning question at
  all — it is sticky placement, a scheduling decision.
- **`fetch` is PINNED, always.** Its checksum is a required argument, so its value
  identity is known before evaluation; the URL is a *provenance coordinate*. A
  read whose result identity is unknown until performed is a DIFFERENT PRIMITIVE —
  an **observation**. One function may not be hermetic-or-discovering depending on
  whether a parameter is present (Amos). `r[machine.receipt.fetch-observation-pin]`
  was misnamed: it describes observation, not fetch.
- **Capabilities are referenced BY IDENTITY** (`primitive.md:9` already said so).
  `Rustc::acquire(spec)` opens no binary — nothing in-program evaluates. It NAMES
  one. Acquiring OUTSIDE a `place` is therefore required, not merely allowed: the
  recipe pins one toolchain identity and every executor materializes *that* one.
  Acquire inside, and each node picks whatever it has — reproducibility dies.
  Demanding a capability no executor can satisfy fails at planning time.
- **`Target::host()` is DEAD** (8 sites in crate.vix, 1 in rodin.vix). Three
  machines wore one word: **target** (semantic — it changes the artifact), **host**
  (cost-model), **executor** (cost-model). Setting the target from the executor
  reads the machine into the value: same recipe, two artifacts, content addressing
  dies. The host is not a fact a program may read; it is an **input the demand root
  supplies**. `vx build --target` is defaulted to the host BY THE CLI, which is
  outside the program.
  > **An ambient read is an observation. An input is a pin.**
- **`place` is a strong boundary**, and this is the restriction that makes a
  distributed demand graph analyzable rather than terrifying:
  > **A value may cross a `place` boundary only if its identity is known without
  > evaluating it.**
  A pinned blob (checksum in the source), a capability identity, a literal, an
  input pinned at the demand root: all cross. `let x = expensive();` does not —
  knowing what `x` *is* means computing it. Either compute it first, or draw the
  `place` wider. Before dispatch you know exactly what crosses and what it weighs.
- **Where does the fetch happen? On the executor.** Nothing outside the `place`
  demands the bytes; the only demand for them is the `exec`, which runs there.
  32 bytes of identity cross. Your machine never downloads the tarball it is
  compiling. This is entirely the pin's doing: strip it and the identity does not
  exist to cross with, so the observation must be performed — and **an observation
  made inside a `place` was made by someone else.** Its pin becomes your receipt's
  authority and there is nothing to check it against.
  > **Placement is trust-free exactly when everything inside it is content-addressed.**
- **A tree crosses as an identity plus a mount grant.** Nothing is copied. Blobs
  materialize per-file on read, by content hash; every read AND every miss is
  recorded (`witness-reads`; absence-is-an-observation). A workspace of 10,000
  files whose compiler opens 200 moves 200. Edit the README: the tree's hash
  changes, the recipe identity changes, and **nothing reruns** — because the memo
  is indexed by LOCATION, which is content-free, and the entry carries a read-set
  the README is not in. The unused half of a tree costs nothing at every level:
  no transfer, no invalidation, no receipt entry, no attestation. This question is
  *why the location plane exists*.
- **Killing a process early is not a scheduler feature.** If you demanded a unit's
  `.rmeta` and never its `.rlib`, the rest of that process's output is undemanded.
  Stopping it is the laziness law arriving at a subprocess boundary, and the
  demanded projection is bit-identical either way. The kill must be driven by the
  demanded projection, not by a scheduler's judgement — otherwise a scheduling
  artifact enters a value's identity.
- **Command grammars are `name!{ … }`.** Backticks are string templates, not a
  command form.

**Consequences landed this round.**
- Book: `std/collections.md` rewritten (one structure, four names; streams;
  structural order + `Order<T>`; `Multiset` deleted). `testing.md` rewritten to
  generators + phase-partitioned `Check`. New chapter `effects.md` (w32).
  `building-a-solver.md` de-Multiset'd. Reading order fixed (effects w32,
  testing w35 — both use `Stream`/`Map` from collections w30).
- Spec: `machine.primitive.fetch-is-pinned`, `machine.primitive.capabilities-by-identity`,
  new `spec/machine/placement.md` (`value-irrelevant`, `identity-crosses`,
  `no-in-program-steering`, `capability-requirements-are-derived`,
  `trees-cross-as-grants`, `kill-is-laziness`, `observation-inside-a-place`),
  `machine.identity.merkle-tree`, `machine.identity.streams-cross-island-edges`.
  `machine.receipt.fetch-observation-pin` renamed in place.
- `vix/corpus-next/SURFACE.md` v2 is the porter's law.

**STILL OPEN.**
- Does the **AST** travel, or the **lowered island**? (Executor hosts `vixc` or
  only `weavy`? Decides whether an executor can be a static binary on a machine you
  do not administer — the sovereignty story.)
- Does **codata cross a `place` boundary**, or only an island edge?
- The **observation primitive**: name and shape. Until it lands, checksumless
  retrieval has no surface.
- **stdout has no home.** Three corpus sites route it through a fake file
  (`--stdout {p"cfg.stdout"}`). The effect model must answer this.
- **The failure surface** (`fail`): three ports weaponized `.get().unwrap()` on an
  empty map for want of it. Queue item C3.
- The molten-vs-codata edge asymmetry.

### Round 10 addendum — strings and commands (Amos: his original design, restored)

- **Backticks are COMMANDS**, a tagged template whose tag is a **capability value**:
  `` exec rustc`-c {src} -o out` ``. Interpolation inside a command is `{expr}`, and
  an interpolated value is an argv *element*, typed.
- **Strings follow bash**: `"…"` interpolates with `${expr}`; `'…'` is always
  literal.
- This SUPERSEDES round 6's interpolation ruling (backtick templates, `""` literal).
  Round 6's reason was the uncanny principle — a Rust reader's pasted string must
  never change meaning. That trap is now **accepted, not dissolved**:

  > **LEDGER (innovation points).** `"${HOME}"` pasted from Rust changes meaning.
  > Defended by: rarity (`${` in a Rust literal is vanishingly uncommon), by bash
  > familiarity (this is the single most-read string convention on earth), and by
  > the `'…'` escape hatch. Accepted deliberately. Observable at a diagnostics rung.

- **`name!{ … }` was wrong, and wrong structurally**, not just aesthetically: a
  macro cannot refer to the capability you just bound.
  ```vix
  let rustc = Rustc::acquire spec;   // a VALUE: a pinned toolchain identity
  exec rustc`-c {src} -o out`        // the value is the tag
  ```
  A command is not a free-floating string that happens to name a program. It is an
  argv addressed to a toolchain you already pinned. `exec cc!{ … }` in the v1
  corpus is the old shape.
- OPEN: does `p"…"` interpolate (following `"`), and does `p'…'` exist?
- OWED: a mechanical sweep of `vix/corpus-next/*.vix` (`rustc! {`, `build_script! {`)
  and `vix/tests/ratchet/*.vix` (`exec! {`) onto the tagged-template form.

### Round 10 — the critic pass (three seats, and what they cost)

Three critics read the book, the spec, `SURFACE.md`, the design memos, and the four
re-ported programs. All three found real defects. The pattern below matters more
than any single fix.

**THE PATTERN: a conclusion outliving its reason.** Named twice during the session,
found three more times by the critics, and committed once *inside the fix for it*:

1. `machine.identity.tier-not-in-hash` [SETTLED] asserted that a `Pending<T>` and
   its realized value "share declared identity" — directly contradicting
   `machine.identity.pending-identity`, eight lines below, which says they do not.
   The round-5 reconciliation identified this exact clause as the two planes smeared
   together and ordered it deleted. The sweep hit two of three sites. STRUCK now.
2. `machine.identity.zero-padding` [SETTLED] still concluded "flat-byte hashing is
   valid unconditionally" — the conclusion of `canonical-memory`, which is STRUCK,
   and which `framed-encoding` [SETTLED] bans. Zero padding is hygiene. It licenses
   no hashing. CORRECTED. The book's FOUNDATION chapter was teaching the struck
   definition verbatim (`_index.md`: value identity as `(schema, blake3(memory))`).
   CORRECTED.
3. And the one that stings: an earlier round-10 commit "fixed" `carried-hasher`
   resting on the OPEN `map-order-independence` by **removing the citation and
   keeping the conclusion it had been citing it for**. Deleting a citation while
   retaining what it justified is the same defect wearing a bandage. CORRECTED —
   `carried-hasher` is now scoped to ordered aggregates and says nothing about maps.

**THE MEMO HAD TWO INDEXES.** `machine.memo.demand-key` keys the memo by a digest
over argument CONTENT hashes. `three-planes.md` argues, by name, that content-keying
is the bug — "the content changed, the key changed, yesterday's entry is filed under
a name we no longer know." Round 5 promised a location rule and never wrote it, so
`placement.md`'s headline claim ("changing a file nobody read invalidates nothing")
had nothing behind it. NEW: `r[machine.memo.indexed-by-location]`. The index is the
location; `DemandKey` demotes to a **field of the entry**.

**THE TOTALITY PROOF RESTED ON NOTHING.** `by_key(f)` is "total by construction,
ties broken by the structural order of `x`" — true only if `x` has one, which the
spec never defined for maps, sets, floats, closures, blobs, or recursive values.
NEW: `r[machine.value.structural-order]`, with every base case. It forced two facts:
**a stream is not a value** (so "every value is ordered" never included it), and
`<=>` **must short-circuit on identity**, or `by_key(|x| x.big_tree)` is total and
unusable.

**`Check`'s phase partition had no type.** The harness must tell `expect_eq` from
`never_demanded` and no signature said how. `Check` is a two-variant enum; trace
checks take `Demand<T>` — which is what an undemanded expression already is.

**THE CALLING CONVENTION WAS TAUGHT NOWHERE.** Zero grep hits for "named argument",
"juxtaposition", "positional argument" across eight chapters — while five of eight
wrote code that would not parse, including the capstone. NEW chapter `calling.md`
(w7); `values.md`, `demand.md`, `testing.md`, `building-a-solver.md` swept onto it.

**Smaller, all real.** `m[k]` on a map appeared in the capstone and exists nowhere
else (book or 6,000 ported lines) — deleted. `.unwrap()` appears 135 times in the
corpus and had no page — documented, as a typed demand failure. "Most things are
streams" is refuted 16:1 by the corpus — the claim is now scoped to *nothing from
outside arrives as an array*. Three voice violations stripped. And a fix committed
during this session (generator streams keyed by yield-site ordinal) **collided under
recursion** and contradicted `testing.md` in the same breath — re-keyed by location.

**Exposed by the backtick sweep, unfixable without a ruling:** twelve rungs run
processes with no capability (`exec! { echo … }`), and nothing says how a `#[test]`
obtains one. Rung 070 exists to reject an undeclared capability — but under tagged
templates that rejection **falls out of scoping** (an undeclared capability is an
unbound identifier), and the ratchet contradicts itself: rung 067 runs `echo`
undeclared and is expected green. Logged in `vix/tests/ratchet/PORT-NOTES.md`.

### Round 11 — failure, tuples, test capabilities (Amos)

- **Tuples satisfy at-most-one-positional VACUOUSLY**, and that is both the danger
  and the answer. `f (a, b)` passes ONE argument: a struct whose fields are named by
  position (the same sentence that makes an array a map). So the rule stands and
  tuples are its honest escape hatch: right when the arguments *together form a
  value* (a pair, a span, a coordinate); **wrong when they have ROLES**, because a
  tuple has no room to name them and the swap bug returns. `expect_eq (actual,
  expected)` is therefore still wrong; the answer is a name or a receiver.

- **`fail` — archaeology confirms Amos's memory.** `tree-sitter-vixen/grammar.js:729`
  has `fail_expression: seq("fail", $.expression)`; `:691` has a postfix `?`
  (`_immediate_question` — must touch its operand); `vixen/docs/design/vix-spec.md:165`
  V28 is "advertise ⇒ watch ⇒ poison".
  - **`fail <payload>` makes a demand have no answer.** A failure is NOT a value; it
    is the absence of one. You supply a typed payload. The machine attaches the
    subject's identity, the source span, and the **demand chain** — read from the
    live demand map at failure time. You cannot forget them because you never attach
    them (`r[machine.error.carries-context]`, SETTLED).
  - Amos: "fail as a string is kinda weak. A struct is not much better. We want the
    full demand at least." Exactly — and that is already the settled rule.
  - **Poison is per-demand, not global.** 200 compiles, one fails: the other 199 keep
    their values, receipts and memo entries; only the link is poisoned.
    `--keep-going` reports all, for the same reason a test reports every failing
    check rather than the first.

- **CORRECTION to Amos's recollection: `?` cannot yield `Option`.**
  `r[machine.error.option-not-channel]` [SETTLED]: *"`Option` is not an error
  channel. Fallible operations return `Result`; absence-as-failure erases the
  failure's address by construction."* That erasure IS the governing incident — a
  solve failing with `"unwrap on None"`, no location, no subject, no demand chain.
  A `?` that turns a rich `Failure` into `None` throws away exactly what the failure
  carried, at the moment someone decided to look.
  > **`expr?` yields `Result<T, Failure>`.** `expr?.ok()` if you truly don't care why,
  > and it says so in the source.
  - `?` is *catching* here, not propagating. It has to be: propagation is the
    default (a failed demand poisons its dependents whether or not anyone writes a
    symbol), so the operator's job is the opposite of Rust's.
  - `Result<T,E>` is for outcomes a caller branches on (an UNSAT solve with its
    derivation is an *answer*). `fail` is for absence.
  - `o.unwrap()` IS `match o { Some(v) => v, None => fail UnwrapOnNone {…} }`
    (`r[machine.error.option-unwrap-span]`). `m.get(k).unwrap()` as an error-raise —
    which three ports independently invented — becomes `fail MissingKey { key }`.
  - **Queue item C3 is CLOSED.** New chapter: `/vix/errors` (w22).

- **A test's parameters are what the harness supplies** (Amos). A test that runs a
  process needs a capability; a program may not go looking for one. So it declares:
  ```vix
  #[test] fn exec_echo(sh: Sh) -> Stream<Check> { let out = exec sh`echo "hi"`; … }
  ```
  The harness IS the demand root, so this is `r[machine.placement.no-in-program-steering]`
  exactly — the same act as `vx build --target` defaulting to the host. An ambient
  read is an observation; an input is a pin. The harness may also **forge** a
  capability (a fake `Sh` returning fixtures), which is how you test exec hermetically.
  - **Rung 070 dissolves.** `exec cc`…`` cannot resolve `cc` unless `cc: Cc` is a
    parameter, so an undeclared capability is an **unbound identifier**. The bespoke
    "undeclared capability" check was a workaround for a language that couldn't say it.
    The ratchet's self-contradiction (rung 067 runs `echo` undeclared and is expected
    green) resolves the same way: 067 declares `sh: Sh`.

**Owed:** sweep the twelve `exec! { … }` rungs onto `#[test] fn f(sh: Sh)` + tagged
templates; delete or re-diagnose rung 070; port `.get().unwrap()` error-raises in
`crate.vix` to `fail`.

### Round 11 addendum — commands, closures, and why we are not Nix

- **A command is a tool projected out of a CLOSURE**, plus a typed argv. Not a tag
  naming an executable: **executables are seldom self-contained** (`cc` is `cc1` + a
  specs file + a libc + a linker + headers). A projection carries its closure, so
  `c.cc` and another toolchain's `c.ar` cannot be paired — you cannot write it down.
  This is `r[machine.primitive.exec-probed-toolchain]`'s "the TOOLCHAIN's probe output
  enters exec identity", made structural.
- **The closure obligation has exactly two discharges** (`r[machine.capability.two-classes]`
  restated as a duty): **materialized** (complete CAS description of every runtime
  dependency; portable to a machine that has never seen it) or **ambient** (on the
  local filesystem, and the daemon guarantees it does not shift: advertise ⇒ watch ⇒
  poison).
- **CORRECTION: there is no stable global path, and we do not need one.** An earlier
  draft demanded a Nix-style `/nix/store/<hash>` namespace so RPATHs would resolve
  everywhere. That is Nix's answer to Nix's constraint, imported without its premise.
  Amos: Nix packages EXISTING software and may not change it, so the immutable global
  path is its only lever. **We maintain patch sets.** If a program `dlopen`s, it does
  so from a path *we arranged*. And a global path is unavailable anyway: unprivileged
  installations have no `/nix` to create, and the mount root differs on Windows, Linux
  and macOS — Linux does not care about this; we have to.
- **Nix is files-in, files-out, bash in the middle. We have types and parsers.** We
  know a compiler emits an object file and a linker emits an executable. So a produced
  executable's closure is **analyzed**, not scanned for a magic prefix:
  `ArtifactFacts { format, needed, rpath, interp, exports }` — a typed value with a
  content hash that enters the command's identity. Not speculative:
  `vix/src/reloc_selection.rs` already parses Mach-O with `object` (sections, symbols,
  relocation walks) for test selection. Binary-snark is the same job, said better.
- **THE CLOSURE OBLIGATION IS A TYPE.** A produced executable's command is a projection
  of the **exec outcome**, never of the tree:
  ```vix
  let built = exec rust.rustc`--crate-type bin {src}`;
  exec (built.artifact p"build_script")`--out-dir {out}`;
  ```
  `tree / p"build_script"` is a `Blob` — bytes, which cannot promise anything. The
  facts were gathered at the end, by the thing that produced it. `crate.vix` tagging a
  command with a `String` does not typecheck, not because we disapprove but because a
  `String` never knew what it depended on.
- **The read-set is necessary and NOT sufficient**, and the gap is the invariant. On
  Windows the linker creates dependencies on system libraries outside the VFS; no
  read-set will ever name them. The artifact analysis does.
  > Every dynamic dependency of a produced artifact must be either **(a)** in the
  > producing exec's read-set (materialized, ours, identity known) or **(b)** covered
  > by an advertised ambient capability. Anything else is a hermeticity hole, and it is
  > **detected at production time.**
  Nix detects that hole at *runtime*, on someone else's machine, as a missing shared
  object. That is the whole of the gloat, and it is narrow enough to be true.
- **The dynamic aspect is arranged, not inferred.** Plugins and `dlopen` of a
  runtime-assembled path defeat analysis and byte-scanning alike. They belong in the
  per-platform build files we provide: the recipe declares where plugins live, the
  closure includes that directory, and a `dlopen` outside it is an undeclared read —
  a loud failure, not a silent success.

**Full note:** `/vix-design/runtime-closures`. Open there: mount layout and read
attribution once loader paths are ours; whether ambient closures are the ONLY thing
that makes an exec unplaceable; what a forged capability is for a test; and where
patch sets live (they should be ordinary content-addressed inputs — say so).

### Round 11 — patch sets (Amos): "Patchsets live in the package manager. As vix packages."

Not a mechanism. A **package**. Applying one is therefore **resolution**, not a
build-system feature: the Gentoo-inspired overlay system already designed for the
registry *is* the patch mechanism ("an internal overlay so it can patch any package, at
will, invisibly to build logic"), and rodin already resolves against the composed view.
Receipts already name the overlay that supplied each package, so patch provenance is
free. The package brief already wrote the other half: *"the whole pancake stack is just
more content-addressed inputs. rustc, linker, sysroot: hashes in the graph."*

> **A materialized toolchain is a resolved package graph.** A patched GCC is
> `upstream@hash` + `patchset@version`, composed by an overlay, resolved by rodin.

Two collapses:

- **There is no capability system for materialized toolchains.** They are packages.
  Only AMBIENT toolchains need capabilities — which is what
  `r[machine.capability.two-classes]` has been saying, now meaning something operational.
  OPEN: does `Rust::acquire` survive at all? Depending on a package is depending on a
  package.
- **Placement is unconstrained except by ambient closures.** Materialized closures are
  blobs; any node that can fetch can run them. `r[machine.placement.capability-requirements-are-derived]`
  amended to say exactly this, and it is now one sentence instead of a taxonomy.

**And the hole this opens: THE SEED.** A binary embeds its recipe; the recipe names its
toolchain; the toolchain is a package built by a toolchain. The chain bottoms out at a
binary with **no recipe**. Content addressing does not answer this — every byte of the
seed is content-addressed and equally unexplained.

**Amos: this is not a place we are behind; it is a place to join forces.** The
Bootstrappable Builds community already solved it (`hex0` → `M2-Planet`/`Mes` → `tcc` →
`gcc`; Guix shipped a full-source bootstrap on it; `live-bootstrap` assembles a userland).
We reuse their work: the seed is a pinned `fetch`, small enough to audit by eye, and the
chain is an ordinary vix recipe.

What we hand back is not nothing. Their chain becomes reproducible by **observation**
rather than by convention — every step an exec with a witnessed read-set, so "this gcc
descends from that seed" is a **receipt chain**, not a claim about a build script. `snark`
is ours and bootstrap work drowns in parsing (ELF, ar, tar, M2-Planet's C subset). And the
transparency log is exactly the home a full-source bootstrap's attestations lack.

Then the recipe-embedded artifact's leaves are: source tarballs (pinned) and a tiny,
human-auditable seed (pinned). **No unexplained binary anywhere in the graph** — the
strongest form of "rebuild a binary by virtue of having the binary", reachable because
someone else already did the hard part. Brothers in arms, not rivals.
(Attribution/figures from memory; verify before they go anywhere public.)

### Round 12 — exec/place decoupled; Tree is not a flat map; fetch's two hashes

**1. `exec` and `place` are decoupled, and neither mentions the other.**
`r[machine.primitive.exec-is-placement-agnostic]`. `exec` returns an ordinary struct
(`r[machine.primitive.exec-outcome]`):
```vix
struct ExecOutcome { tree: Tree, stdout: Stream<Int,String>, stderr: Stream<Int,String> }
```
No observer parameter. **Stream processing happens remotely by PLACING THE SURROUNDING
BLOCK**, not by handing a closure to `exec`.

> **The observer closure is NOT obsolete. It is the lowering.**
> `vixen/docs/design/vix-language-design.md` §"What ships to executors" already says the
> executor receives "the canonical AST of the CLOSURE … plus an OBSERVER CLOSURE holding
> the process handle, able to return anything incl. streams." That is exactly the lowering
> of a placed block over exec's codata fields. **Retired: the observer as a *surface*
> construct.** `exec cmd where { observer: … }` does not exist. `/vix-design/exec-observers`
> keeps its findings (readiness is a protocol fact, not a filesystem fact) and loses its API.
> Readiness authority = the placed block reading a stream the tool controls. A subfile
> projection resolving early is the CONSEQUENCE. For a **protocol-less** tool the safe
> authority is **process exit**: a VFS close event is NOT sound (a process may close, reopen
> and mutate). A close event is admissible only when the command grammar *promises*
> monotonic/close-final outputs. And "rustc announces artifacts on stdout, which is how
> cargo pipelines rmeta" is **unsourced** — the argument does not need it; do not repeat it.

- **No exit status.** An exit code is a naked `Int` where a typed outcome belongs. Nonzero
  is a `fail`. Where nonzero is a legitimate answer (`grep` → 1), the answer could only
  come from the **command grammar** — but **how an accepted status becomes a typed result
  is OPEN and blocking**, because `ExecOutcome` carries no status and `Match`/`NoMatch`
  would be indistinguishable. Today a grammar may declare only which statuses **fail**
  (`r[machine.primitive.exit-status-is-not-a-value]`).
- **A stream MAY be a record field** — retracting an overclaim. Its semantic content is the
  value it drains to (`streams-cross-island-edges`: a field is an edge).

**2. `Tree` is not `Map<Path, Blob>`.** That was an oversimplification and it loses
directories (especially empty ones), symlinks, and the executable bit — all of which the
store models today. `r[machine.identity.tree-model]` mirrors `vx-services`'s
`DirectoryEntryKind`:
```vix
Tree      = Map<Name, TreeEntry>              // ONE path segment, recursive
TreeEntry = File { content: Blob, executable: Bool } | Dir (Tree) | Symlink { target }
```

> **`TreeHash` is NOT Vixen's storage `NodeHash`** (`r[machine.identity.tree-hash-is-not-node-hash]`).
> `SmallFile`/`LargeFile`, `blob_node`, chunk boundaries and `total_size` ("for progress
> reporting only") are **storage representation**. **Rechunking must not change a Tree's
> semantic identity**: a file crossing `CDC_MAX_CHUNK` is the same file, and a store that
> changes its chunker must not invalidate one memo entry. Same disease as `canonical-memory`
> (ABI into identity) and `ExecTree`'s UTF-8 split (a representation predicate into the
> schema). Whether the two hashes could share a canonical preimage must be **proved**.

**OPEN, surfaced not decided** (`r[machine.identity.tree-canonicalization]`): entry order
(store sorts by `name.as_bytes()`; vix's `String` order is by scalar value — these coincide
for valid UTF-8, which is also the store's constraint: say so or say they diverge); whether
`executable: Bool` is the model on every platform and what it means on Windows; mtime /
uid+gid / xattrs / hardlinks / device nodes / ACLs / name case-sensitivity as explicit
non-goals or round-trip requirements; symlink targets absolute-or-relative and may they
escape the tree (they interact with `trees-cross-as-grants` and with relocation); empty
directories must round-trip (`mkdir -p out` before a compiler runs).

**3. `fetch` returns a `Blob`, never a `Tree`** (`r[machine.primitive.fetch-returns-a-blob]`).
`extract blob -> Tree` is a separate demand, and **an archive-byte digest is not the
resulting tree's digest**: two archives differing in compression or member order may unpack
to one tree — one `TreeHash`, two `ContentHash`es.

**One name, one optional provenance check** (`r[machine.primitive.fetch-integrity-vs-identity]`):
- **`blake3` — the vix ContentHash — is REQUIRED.** It is the value's name in the one
  identity space. Computing it for an upstream artifact is a **lock-time** act: fetch once,
  hash, write it into the recipe. Every build thereafter knows the final `Blob` identity
  before evaluating anything, so **every `fetch` crosses a `place` boundary by construction**.
- **`sha256` is OPTIONAL transfer provenance** on the bytes that arrive. It never becomes an
  identity: a value must not be named in a hash family chosen by whoever hosted it.
- **There is no SHA-only fetch.** An operation whose result identity is unknown until the
  bytes arrive is an *observation*, not a fetch. `fetch` may not become
  hermetic-or-observational depending on whether an optional field is present — the exact
  defect Amos rejected in round 10.

**4. CORRECTION to `r[machine.placement.capability-requirements-are-derived]`.** The claim
that a materialized closure "constrains placement not at all" was **false**. Materialization
removes **locality**, not **platform compatibility**: a content-addressed `x86_64-linux`
binary still executes only on `x86_64-linux`. Two independent axes — an **execution-platform
contract** (both materialized and ambient closures impose it) and **host-specific locality**
(ambient only — and an advertised fingerprint admits the matching **set** of nodes, not one
host). The old wording would have let the scheduler dispatch a Linux `rustc` to a Mac. Say
"satisfies the execution contract", not "is of that platform": a node may satisfy
`x86_64-linux` by emulation.

**Three things, not two.** (1) The **target** — what the artifact is for. Semantic.
(2) The **selected toolchain's host / execution ABI**, including Cargo's `HOST` — a pinned
semantic property of the toolchain (it enters exec identity) AND a scheduler admissibility
constraint. **Not cost-model.** (3) The **physical executor** — cost-model, unobservable,
absent from the semantic receipt. An earlier draft collapsed (2) into (3).

**Directionality** (`r[machine.placement.results-cross-back]`): `identity-crosses` governs
**dispatch** — captured/imported values must have an identity known without evaluating the
block. It says nothing about **results**: a placed block's derived value is computed remotely,
acquires its identity there, and crosses back. A placed block consuming `out.stdout` and
returning a diagnostic violates nothing.
