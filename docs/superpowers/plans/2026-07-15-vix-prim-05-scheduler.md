# Vix Typed Primitives — Phase 05: Scheduler Effect Resolution Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:test-driven-development per task. Steps use checkbox (`- [x]`) syntax.

**Goal:** Wire effect resolution into `Runtime::evaluate`'s input assembly (design §7). For each effect edge on a consumer island's `LoweringArtifact.effect_inputs`: evaluate the request island (ordinary pure demand) → request `ValueId`; build the effect `DemandPreimage { closure: primitive recipe, arguments: [request] }` → `DemandKey`; consult the memo **policy-aware** (Volatile skips lookup+insert; Hermetic normal path; Pinned/Observed honest `MachineError`); on miss create the demand record, mint the `EffectTicket` on it, call `primitive.begin(request_ref, &mut ctx)`; v1 completes synchronously inside `begin` (the phase-02 adapter calls `ctx.complete`); handle the `Completion`; bind the interned response at the artifact's `entry` slot as an ordinary realized value input; spawn the consumer.

**Architecture:** demand-level dispatch, zero weavy changes (design §"demand-level dispatch"). Effect resolution rides the same value-input realized channel already used at task spawn. The **shared value-input binding** duplicated across the value-island lane (`evaluate`) and the generator lane (`drive_generator`) is extracted FIRST into one behavior-preserving helper, so the effect response binds through the exact same path.

**Tech Stack:** Rust (edition 2024), `vix::runtime::{scheduler, identity, error, observe, model, store, primitive}`, `vix::lowering`.

## Global Constraints

- Branch `vix-prim-05-scheduler` (git town append from `vix-prim-04-lowering`). All commits `git commit --no-verify`, no AI attribution.
- **Layering:** the artifact carries `vir::EffectId`; the scheduler converts `EffectId → PrimitiveId` at the runtime boundary via `PrimitiveSet::by_effect_id`. Never leak `PrimitiveId` back into `vir`/`lowering`.
- **One generic effect path** (r[machine.primitive.registered]): one memo-key construction, one dispatch site, one new `MachineOperation::Effect`, one generic `FailureValue::Primitive` variant, one effect event/counter parameterized by `PrimitiveId` as DATA. No per-primitive arms/fields/variants.
- **v1 memo policy = Volatile + Hermetic only.** Pinned/Observed are an honest typed `MachineError` ("policy not supported in v1"), never silently Hermetic.
- **Completion (design §7.4):** `Ok(interned)` → memo entry with populated `Receipt`, bind at `entry`, Ready. `Failed(PrimitiveFailure)` → generic `FailureValue::Primitive`, memoized per policy with receipt, propagated through the existing `Evaluation.failure` path. An `EffectProtocolError` (the begin/finish Result channel) → `MachineError` via `MachineOperation::Effect` + `RuntimeFault::Effect`, never memoizes. Re-entrancy trips the existing `ReentrantDemand` guard.
- **No clock/polling** (r[machine.scheduler.block-on-event]): ticket owned by the demand record; completion arrives synchronously inside `begin`.
- **`EffectCtx` is the primitive's only window** — the phase-02 surface (`witness_read`/`emit`/`complete`/`store_mut`/`finish`) is wired as-is; the adapter already interns the response through `store_mut`.
- **No pure-path regressions.** An artifact with empty `effect_inputs` hits ZERO new code. Full suite green + clippy clean. Every existing `Runtime` is built with no primitives (optional slot defaults empty) → the effect path is never entered.
- **Typed errors only.** No `Result<_, String>`; the three error planes never mix.
- Toolchain: system cargo (rust 1.96). Tests `nix shell nixpkgs#cargo-nextest --command cargo nextest run -p vix`; clippy `nix shell nixpkgs#clippy nixpkgs#cargo-nextest --command cargo clippy -p vix --all-targets -- -D warnings`. The `cross_lane_differential` ~435s test may be slow-killed by nextest under load — re-run in isolation via plain `cargo test`.

## Input contract (phase 04 as landed)

- `PartitionedTest.effect_islands: Vec<PartitionedValue>` — the request islands (pure), already lowered/warm.
- `LoweringArtifact.effect_inputs: Vec<EffectInputBinding>` on the consumer island — `{ primitive: EffectId, request: ValueIslandId, entry, schema, store_schema, payload_element_schema, ty, publication_schemas }`. `entry = value_inputs.len() + k`.
- Phase-02 primitive surface: `PrimitiveSet::{by_id, descriptors, get}`, `Primitive::begin(RequestRef, &mut EffectCtx) -> Result<EffectTicket, EffectProtocolError>`, `EffectCtx::{new, witness_read, emit, complete, store_mut, finish}`, `Completion::{Ok(Interned), Failed(PrimitiveFailure)}`, `PrimitiveId::effect_id`, `MemoPolicy`. **Deviation from design prose:** `begin`/`finish` return `EffectProtocolError`, NOT `Box<MachineError>`; phase 05 lifts it into `RuntimeFault::Effect`.
- **RealizedHandle nuance:** the response binds through the type-driven value-input path — `RealizedHandle` for String/aggregate, `InlineComposite` for a record. Drive it through the shared binding helper; do not assume a store handle.
- **v1 call position:** a primitive is callable only from a test-body value expression; the consumer is a value/check island (`evaluate` lane), never the generator. `drive_generator` gets the shared value-input binding helper but no effect resolution (it has no effects channel in v1); a non-empty `effect_inputs` reaching it is an honest "not supported in v1" guard.

## File Structure

- `vix/src/runtime/identity.rs` — `RecipeId::from_primitive_digest` (domain-separated effect recipe).
- `vix/src/runtime/error.rs` — `MachineOperation::Effect`; `RuntimeFault::Effect { primitive, error }`; `RuntimeFault::UnsupportedMemoPolicy { primitive }`.
- `vix/src/runtime/model.rs` — `FailureValue::Primitive { recipe, site, failure }` (one generic variant).
- `vix/src/runtime/store.rs` — `failure_node` arm for `FailureValue::Primitive`.
- `vix/src/runtime/observe.rs` — `EventKind::EffectDispatched { key, primitive: PrimitiveId }`.
- `vix/src/runtime/primitive/register.rs` — `PrimitiveSet::by_effect_id`.
- `vix/src/lowering.rs` — `RealizedBinding` view + `ValueInputBinding::realized()` / `EffectInputBinding::realized()`.
- `vix/src/runtime/scheduler.rs` — `Runtime.primitives: Arc<PrimitiveSet>` + `with_primitives`; `IslandInputs.effects`; `EffectDemand`; the shared per-entry binding helper `bind_realized_entry` + `EntryBindError`; the effect-resolution core in `evaluate`; `frozen_to_weavy`/`publication_schema`/`frozen_inline`/`frozen_product` retargeted to `RealizedBinding`.
- `vix/src/ratchet.rs` — `IslandInputs { effects: &[] }` at the three literals (effect-inert; the ratchet compiles with no manifest).
- `vix/tests/{force_on_park.rs}` — `effects: &[]` on the four literals.
- Create `vix/tests/primitive_scheduler.rs` — end-to-end resolution tests.

---

### Task 1: behavior-preserving shared value-input-lane extraction (no effects)

Extract the per-entry realized-input binding (the frozen/handle write + the payload_element_schema override) into one helper both lanes call. The two lanes differ ONLY in `terminate_machine_fault` vs raw-return on two unreachable machine-invariant paths (schema-mismatch, missing-value-input-handle); preserve each lane's exact policy via a typed `EntryBindError { Invariant | WriteFault | Raw }` the caller dispatches. `frozen_to_weavy` & friends retarget from `&ValueInputBinding` to a `RealizedBinding` view so the same code binds an effect response later.

- [x] Step 1: no new test — this is a pure refactor; the full suite is the oracle.
- [x] Step 2: implement `RealizedBinding` + `realized()` (lowering.rs); retarget `frozen_to_weavy`/`frozen_inline`/`frozen_product`/`publication_schema`; add `bind_realized_entry` + `EntryBindError`; replace both lanes' first zip loop with the helper preserving exact policy; keep the second (override) zip as-is or fold into the helper's returned override.
- [x] Step 3: `cargo nextest run -p vix` → byte-for-byte green (cross-lane isolated if slow-killed). Clippy clean.
- [x] Step 4: commit — `vix: extract the shared realized value-input binding across scheduler lanes`.

### Task 2: thread `PrimitiveSet` into `Runtime`; effect identity + demand key

- [x] Step 1: failing test (`primitive_scheduler.rs`) — `Runtime::new(sink).with_primitives(Arc::new(set))` builds; `by_effect_id(descriptor.id.effect_id())` finds the primitive; `RecipeId::from_primitive_digest` is stable + distinct from a pure recipe.
- [x] Step 2: `Runtime.primitives: Arc<PrimitiveSet>` (default empty) + `with_primitives`; `PrimitiveSet::by_effect_id`; `RecipeId::from_primitive_digest`; `IslandInputs.effects: &[EffectDemand]` + `EffectDemand`. No dispatch yet.
- [x] Step 3: full suite green (empty registry + `effects: &[]` = no-op). Commit — `vix: thread a PrimitiveSet registry and effect identity into the runtime`.

### Task 3: the resolution core in `evaluate`

For each `(binding, effect_demand)` in `lowered.effect_inputs.zip(inputs.effects)`: evaluate the request island → request `Evaluation`; if it failed, the consumer fails with that handle/failure. Else build the effect `DemandPreimage`/`DemandKey`; policy-aware memo (Volatile skip; Hermetic lookup; Pinned/Observed honest error). On hit: response = memo handle. On miss: clone the request frozen, `EffectCtx::new(&mut store)`, `begin`, `finish`; `Ok(interned)` → `effect_spawns++`, `EffectDispatched`, memo insert with receipt, Ready; `Failed` → intern the `FailureValue::Primitive`, memo per policy with receipt, consumer fails; `EffectProtocolError` → `RuntimeFault::Effect` abort. Then bind each resolved response at `binding.entry` via `bind_realized_entry` inside the task-setup loop.

- [x] Step 1: failing e2e tests (below).
- [x] Step 2: implement `resolve_effects` (called before the chaos respawn loop) returning bound responses or a consumer failure; bind responses in the task loop.
- [x] Step 3: green + clippy. Commit — `vix: resolve registered effect primitives at the demand layer`.

### Task 4: integration tests + phase gate + landing notes

Tests (`primitive_scheduler.rs`), compile+lower a real primitive and drive `Runtime::evaluate` directly (mirroring `force_on_park.rs`):
- Hermetic: two identical demands → second is a memo hit, **zero** extra `begin` (AtomicUsize counter).
- Volatile: two demands → **two** `begin`, no memo entry.
- Failure: a `PrimitiveFailure` propagates as `Evaluation.failure`, receipted, memoized per policy.
- Receipt: an Ok effect's memo entry carries a populated `Receipt`.
- Counter/event: `effect_spawns` increments on dispatch; a hit does not.
- Pure regression guard: an effect-free `evaluate` is byte-identical.

- [x] Full suite green (cross-lane isolated if slow-killed) + clippy clean. Re-read the diff vs Hard Constraints. Append landing notes. Commit — `vix: mark phase 05 scheduler plan complete + landing notes`.

## Self-review notes

- **Effect Location:** the effect memo is keyed by its `DemandKey` (primitive recipe + request value) per r[machine.memo.effect-results]; the location-indexed memo table uses `LocationId(demand_key.0)` so identical effect demands share one cell (unlike pure values, which key by call site). Distinct domain (`vix.demand.v1`) from pure locations (`vix.location.v1`) → no collision.
- **Borrow discipline:** clone the `Arc<dyn Primitive>` out of `self.primitives` and the request `FrozenValue` out of `self.store` before `EffectCtx::new(&mut self.store)`, so the ctx's `&mut store` never aliases the primitive borrow or the request frozen (mirrors the phase-02 unit test).
- **Resolve once:** effects resolve BEFORE the chaos respawn loop; responses bind per spawn. v1 effects run only under non-chaos `evaluate`, so no double-dispatch.
- **String request fields:** `decode_value` rejects `FrozenValue::Reference` (store-resident strings). v1 core tests use scalar-field requests (Int/Bool) that realize inline; a String-request path (reference resolution before decode) is validated as a bonus / deferred to phase 06 if it needs a resolver.

## Landing notes (phase 05 complete)

**Commits (this branch):**
- Task 1 `7319a80b2` — shared realized value-input binding across scheduler lanes (orchestrator-committed).
- Task 2 `ff42fff09` — thread `PrimitiveSet` registry + effect identity into the runtime (orchestrator-committed).
- Task 3 `f7a0a0807` — resolve registered effect primitives at the demand layer.
- Task 4 `8f2772648` — certify effect resolution and the memo fold (`vix/tests/primitive_scheduler.rs`).

**The key correctness insight — fold effect responses into the consumer demand key.** `evaluate` resolves every effect edge FIRST (before `DemandExecution::new`), then builds the consumer's demand arguments as `value identities ++ resolved effect-response identities`. So an effect participates in content-addressed memoization exactly like a value argument: two demands of the same consumer against the same Hermetic effect fold the *same* response identity and hit the consumer memo.

**Dedicated effect memo (supersedes the self-review "Effect Location" note).** The effect result is keyed in a dedicated `effect_memo: BTreeMap<DemandKey, EffectMemoEntry { result: Handle, receipt: Receipt }>`, NOT the location-indexed pure `memo` via `LocationId(demand_key.0)`. The effect `DemandKey` is `from_preimage(closure: RecipeId::from_primitive_digest(id.0), arguments: [request identity])`, under the distinct `vix.recipe.effect.v1` domain — structurally uncollidable with a pure recipe. Only Hermetic populates it; Volatile skips lookup + insert.

**Memo-fold proof (empirical, `primitive_scheduler.rs`):**
- Hermetic: two identical consumer demands → **1** `begin` (AtomicUsize), `effect_spawns == 1`, exactly **1** `EventKind::EffectDispatched`. The 2nd demand re-evaluates the request (pure memo hit), hits `effect_memo` (no `begin`), folds the same response id → consumer memo hit.
- Volatile: two demands → **2** `begin`, `effect_spawns == 2`, **2** dispatch events; `effect_memo` never touched.
- Failed completion → `FailureValue::Primitive { recipe, site }` language failure through the normal `Evaluation.failure` path; 1 `begin`, `effect_spawns == 1`.
- Effect-free `evaluate` → 0 spawns, 0 dispatch events; empty `effects: &[]` folds nothing → byte-identical demand identity (no pure-path regression).

**Deltas from the plan prose (all in-spirit, none touch the memo-fold/effect_memo mandate):**
- `FailureValue::Primitive { recipe: RecipeId, site: u32 }` drops the primitive's `code`/`message` from *identity* in v1 (rendered report bytes are non-identity storage, consistent with the other language failures); `failure_node` tags it `8`.
- One generic effect plane: `MachineOperation::Effect` + `RuntimeFault::{UnregisteredEffect, UnsupportedEffectPolicy, EffectProtocol, MissingEffectRequestFrozen, UnsupportedEffectPosition}`, all keyed by `EffectId` data. Added one defensive `RuntimeFault::EffectInputCardinality { expected, actual }` so a bindings/effects count mismatch can never silently drop an effect and produce a colliding consumer key.
- `resolve_effects` takes no `chaos` param: the nested request island is a pure demand and (like a wire) resolves under `ChaosPolicy::default()`; chaos does not cascade into it.
- `drive_generator` carries an honest `UnsupportedEffectPosition` guard — the generator lane has no effects channel in v1, so a non-empty `effect_inputs` reaching it is a typed error, never silently ignored.
- `EffectDispatched { key, recipe }` carries `RecipeId` (Facet) rather than `PrimitiveId` (not Facet) to keep `EventKind` derivable without leaking a runtime-only identity type.
- Test surface uses an **Int request field + String response** (`probe where { n: Int } -> String`): the Int request decodes inline, and the String response interns and reads back cleanly through the shared realized-binding path. The String-request `decode_value` reference caveat still stands (deferred).

**Pre-existing clippy drift (surfaced, not caused here):** current `nixpkgs#clippy` denies `result_large_err` on Task 1's committed `bind_realized_entry` (its `EntryBindError` carries an unboxed `MachineError`). Line-identical on HEAD; it began firing via registry clippy drift since Task 1 landed. Kept the gate green with a single localized `#[allow(clippy::result_large_err)]` + comment on that cold entry-binding path, rather than reopening the orchestrator-committed helper's shape across ~8 sites. Boxing every `EntryBindError` variant is the mechanical follow-up if the lint tightens further.

**Gate:** `cargo clippy -p vix --all-targets -- -D warnings` clean. `cargo nextest run -p vix` green except the two pre-flagged >4-min tests (`cross_lane_differential::accepted_corpus_agrees…`, `ratchet_runner::accepted_rungs_verify…`), which nextest slow-kills under load; both re-run green in isolation via plain `cargo test` (documented non-regression). `primitive_scheduler` (4/4) green in isolation and in-context.
