# Vix Typed Primitives — Phase 04: VIR Partitioning + Lowering Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Wire the `Op::EffectRequest` node phase 03 emits into the partition/lowering pipeline. Partitioning **cuts at that node**: the request subgraph becomes its own **value island**; the consuming island's node is rewritten to read the response as a **bound realized value input** (`ValueRepresentation::RealizedHandle`). The effect is recorded as one generic edge — `EffectEdge { primitive: EffectId, request: ValueIslandId }` — on the consumer `Island` and mirrored onto the `LoweringArtifact`, so the phase-05 scheduler can resolve it at the demand layer. Phase 04 does **not** resolve, memo, dispatch, or call any handler — it only partitions, rewrites, binds, and records.

**Architecture:** Design §6. `Island` gains `effect_inputs: Vec<EffectEdge>` **parallel to** `wire_inputs`, not conflated with it — `wire_inputs` structurally assumes a *producer* island, an effect has none. The partition cut mirrors the *shape* of the wire cut (a boundary map keyed on node id, a backing vec) but stays a distinct path. The effect-consumer node is rewritten to an `Op::Parameter` (the value-input read whose realized-handle binding already exists), allocated **after** all value-input parameters so the existing `value_inputs`↔`parameters` positional zip in `bind_value_inputs` is untouched; a parallel `bind_effect_inputs` binds the effect params at `entry = value_inputs.len() + k`. The request subgraph is carried in a new `PartitionedTest.effect_islands` (parallel to `wire_islands`) and lowered up front through the ratchet, exactly as wire argument islands are — record/carry only, never resolved here.

**Tech Stack:** Rust (edition 2024), `vix::vir`, `vix::lowering`, `vix::ratchet`, the phase-03 `Op::EffectRequest`/`EffectId`/`EffectKind::Effect`.

## Global Constraints

- Branch: `vix-prim-04-lowering`, created with `git town append vix-prim-04-lowering` from `vix-prim-03-compiler`. All commits `git commit --no-verify` (the facet-dev hook is skipped; never add AI attribution).
- **Layering is load-bearing.** `vir` MUST NOT import `crate::runtime` (`vir.rs` today imports only `crate::diagnostic`/`crate::support` — keep it). `EffectEdge` is defined in `vir.rs` and carries `vir::EffectId`, NEVER `runtime::PrimitiveId`; the phase-05 scheduler converts EffectId→PrimitiveId. `lowering.rs` already imports runtime, but the edge still carries `EffectId` — it originates in `vir` partitioning.
- **One generic effect edge — NO per-primitive arms/fields/variants anywhere** (r[machine.primitive.registered]). Everything is keyed by the `EffectId` data. **Exactly one new edge struct** (`EffectEdge`), **one new `Island` field** (`effect_inputs`), **one new `LoweringArtifact` field** (`effect_inputs`). One new binding struct (`EffectInputBinding`) is the mandatory lowering-side mirror of `ValueInputBinding`, exactly as `EffectEdge` mirrors the `ValueIslandId` carried in `value_inputs`. One new `PartitionedTest` field (`effect_islands`) is the request-island carrier mirroring `wire_islands`.
- **Parallel to `wire_inputs`, NOT conflated with it** (design §6): a separate `effect_inputs` field, a separate `EffectEdge` type, a separate cut loop and boundary-map entry. Do not overload wire code to mean "effect".
- **Phase 04 is lower-only.** No scheduler resolution, no memo lookup/insert, no `begin()`, no dispatch, no `DemandKey` for effects, no receipts/events. Because a primitive call cannot run until phase 05, **tests are structural**: the request subgraph became its own value island; the consumer records the `EffectEdge` with the correct `EffectId` + request `ValueIslandId`; the response binds as a realized value input (`RealizedHandle` for an aggregate/String response); the artifact carries the effect edges. **Do NOT execute a primitive** (no `run_source`/`execute` on an effect-bearing source).
- **No pure-path regressions.** The effect machinery activates only when an `EffectEdge` is present (an effect-free island produces an empty `effect_inputs` and behaves byte-identically). The full existing suite must stay green (516+ tests) and clippy clean.
- **Diagnostics:** reuse existing `DiagnosticCode`/`lowering_diagnostic`; add no new `DiagnosticCode`.
- **Scheduler:** DO NOT edit `runtime/scheduler.rs` for resolution. Only mechanical construction/match updates forced by a new `Island`/`LoweringArtifact` field (e.g. an empty `effect_inputs` in a struct literal). If a scheduler edit would be more than mechanical, STOP and report. (Audited: the only `Island` literals are in `vir.rs`; the only `LoweringArtifact` literals are `lowering.rs:602` and `lowering.rs:246` (`with_test_verified_executable`); `scheduler.rs:3164` returns a `LoweringArtifact` via `with_test_verified_executable`, not a literal — so no scheduler edit is forced.)
- Test runner: `nix shell nixpkgs#cargo-nextest --command cargo nextest run -p vix` (system rustc 1.96.1; do NOT use `nix develop`, it pins 1.91). Clippy: `nix shell nixpkgs#clippy nixpkgs#cargo-nextest --command cargo clippy -p vix --all-targets -- -D warnings`.

## Cut mechanics (verified against the code)

- `collect_dependencies_stopping_at(output, stop)` (`vir.rs:2662`) inserts a boundary node into `needed` but does **not** recurse into its inputs. So adding effect node ids to `stop` includes the effect node in the consumer island while cutting off the request subtree — exactly the required cut.
- `partition_function_output_with_shared` (`vir.rs:1621`) rewrites `shared` nodes → `Op::Parameter` (pushing `parameters` + `value_inputs`) and `wires` nodes → `Op::AwaitWire` (pushing `wire_inputs`). The effect cut adds a **second** loop, run **after** the shared/wire loop, that rewrites `effects` nodes → `Op::Parameter` (pushing `parameters` + `effect_inputs`). Value params therefore occupy `parameters[0..V]`, effect params `parameters[V..V+E]`.
- The contract's `entries` (`lowering.rs:2350`) is `[param_0_region … param_{P-1}_region, constant_regions…]` in `parameters` order. `bind_value_inputs` (`lowering.rs:655`) zips `parameters.iter().zip(value_inputs)` with `.enumerate()` as `entry`; since `value_inputs.len() == V` the zip covers exactly `parameters[0..V]`. `bind_effect_inputs` binds `effect_inputs[k]` at `entry = V + k` (= `island.value_inputs.len() + k`), reading `parameters[entry]` and its `entries[entry]` region — the same schema/region logic as `bind_value_inputs`.
- `representation_for_type` (`lowering.rs:12117`) / `shape_for_type` (`lowering.rs:2465`): `Array`/`Map`/`Set`/`String`/`Path` → single `Handle` word (`RealizedHandle`, `binding.schema` = `Some`); `Record`/`Tuple`/`Enum` → multi-word (`InlineComposite`, `binding.schema` = `None`). An effect param binds through the **same** path as any value input of its type. To make "binds as RealizedHandle" literal and unambiguous, the phase-04 tests use a response type whose top-level representation is `RealizedHandle` (String / Array). A record response would bind as `InlineComposite` — still the realized value-input channel; both are valid and phase 05 delivers accordingly (noted in landing notes).
- After partitioning, an effect node in a **test-body value position** is an `Op::Parameter`, so `lower_node` never sees `Op::EffectRequest` for it. The `lower_node` guard arm at `lowering.rs:5875` stays as a typed diagnostic for the un-partitioned position (an effect embedded in a *callee* is out of scope for v1), with its message updated from "phase 04" to reflect that.

## File Structure

- Modify `vix/src/vir.rs` — `EffectEdge`; `Island.effect_inputs`; `IslandBoundary.effects`; `PartitionedTest.effect_islands`; effect-node collection + `effects` map + `effect_islands` build in `partition_test`; the effect cut loop in `partition_function_output_with_shared`; `effect_inputs` on the three `Island` literals. Inline `#[cfg(test)]`.
- Modify `vix/src/lowering.rs` — `EffectInputBinding`; `LoweringArtifact.effect_inputs`; `bind_effect_inputs`; call it in `lower_island`; field on both `LoweringArtifact` literals; refine the `Op::EffectRequest` guard message. Import `EffectId`.
- Modify `vix/src/ratchet.rs` — lower `partitioned.effect_islands` up front in `prepare_run`, mirroring the `wire_islands` loop.
- Create `vix/tests/primitive_lowering.rs` — structural partition + lowering integration tests (manifest idiom from `primitive_compiler.rs`).

---

### Task 1: VIR — `EffectEdge`, `Island.effect_inputs`, partition cut, request islands

**Files:**
- Modify: `vix/src/vir.rs`
- Test: `#[cfg(test)]` in `vir.rs` + `vix/tests/primitive_lowering.rs`

**Interfaces (later tasks rely on these exact names):**
- `pub struct EffectEdge { pub primitive: EffectId, pub request: ValueIslandId }` — `derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq)`.
- `Island.effect_inputs: Vec<EffectEdge>` — new field, after `wire_inputs`.
- `PartitionedTest.effect_islands: Vec<PartitionedValue>` — new field, after `wire_islands`; each `PartitionedValue { id: request island id, island, wire: None }`.
- `IslandBoundary.effects: &'a BTreeMap<NodeId, EffectEdge>` — new boundary field.

- [x] **Step 1: Failing tests.**
  - Inline `vir.rs`: an effect node in a value expression partitions so that (a) `PartitionedTest.effect_islands` has one island whose output is the request record, (b) the consuming check island's `effect_inputs` carries one `EffectEdge` with the expected `EffectId` and a `request` `ValueIslandId` equal to the request island's id, (c) the effect node in the check island is `Op::Parameter` (not `Op::EffectRequest`), (d) an effect-free test produces empty `effect_inputs`/`effect_islands`.
  - Integration `primitive_lowering.rs`: same, driven through a `PrimitiveManifest` + compiled source (aggregate/String response) — mirrors `primitive_compiler.rs`.
- [x] **Step 2: Verify fail** — `nix shell nixpkgs#cargo-nextest --command cargo nextest run -p vix effect` → compile error (`EffectEdge`, `effect_inputs`, `effect_islands` missing).
- [x] **Step 3: Implement.**
  - Add `EffectEdge` near `Island`.
  - Add `effect_inputs: Vec<EffectEdge>` to `Island` (after `wire_inputs`).
  - Add `effect_islands: Vec<PartitionedValue>` to `PartitionedTest` (after `wire_islands`).
  - Add `effects: &'a BTreeMap<NodeId, EffectEdge>` to `IslandBoundary`.
  - In `partition_test`: collect effect nodes (`Op::EffectRequest`); build `effects: BTreeMap<NodeId, EffectEdge>` (key = effect node id, value = `{ primitive, request: value_island_id(function.id, node.inputs[0]) }`); retain effect nodes out of `shared` defensively; build `effect_islands` (one `PartitionedValue` per distinct request node id, `partition_function_output_with_shared(function, request_node, IslandId(ordinal), Value, &IslandBoundary { shared: &shared_ids, wires: &empty, lazy_arg_reps: &empty, effects: &effects })`, `wire: None`); pass `effects: &effects` in every `IslandBoundary` (values, wire islands, checks, snapshots); set `effect_islands` on the returned `PartitionedTest`.
  - In `partition_function_output_with_shared`: destructure `effects`; add `effects.keys()` to `stop`; after the shared/wire loop add a loop that, for each node still `Op::EffectRequest` whose id is in `effects`, rewrites `node.op = Op::Parameter(ParameterId(parameters.len()))`, clears inputs, pushes a `Parameter { … name: "$effect_…", ty: node.ty }` and pushes the `EffectEdge` to `effect_inputs`; return `effect_inputs` in the `Island` literal.
  - Add `effect_inputs: Vec::new()` to the generator `Island` literal (`vir.rs:1828`).
  - Update the two `canonical_node`-adjacent inline test module and any other `Island {`/`IslandBoundary {`/`PartitionedTest {` literals.
- [x] **Step 4: Run** `nix shell nixpkgs#cargo-nextest --command cargo nextest run -p vix` → PASS (effect-free tests unchanged; new effect tests green).
- [x] **Step 5: Commit** — `git add -A && git commit --no-verify -m "vix: partition cuts at Op::EffectRequest into request islands + effect edges"`

---

### Task 2: Lowering — `EffectInputBinding`, `LoweringArtifact.effect_inputs`, RealizedHandle binding

**Files:**
- Modify: `vix/src/lowering.rs`
- Test: `vix/tests/primitive_lowering.rs` (append) + optional inline

**Interfaces:**
- `pub struct EffectInputBinding { pub primitive: EffectId, pub request: ValueIslandId, pub entry: usize, pub schema: Option<WeavySchemaRef>, pub store_schema: SchemaId, pub payload_element_schema: Option<WeavySchemaRef>, pub ty: Type, pub publication_schemas: Vec<(Type, WeavySchemaRef)> }` — mirrors `ValueInputBinding`, carrying the `EffectEdge` fields instead of `value`. `derive(Clone, Debug, PartialEq, Eq)`.
- `LoweringArtifact.effect_inputs: Vec<EffectInputBinding>` — new field.
- `fn bind_effect_inputs(island, contract, schemas) -> Result<Vec<EffectInputBinding>, Diagnostics>`.

- [x] **Step 1: Failing test** — lower the consumer island (`LoweringCache::default().get_or_lower`): assert `artifact.effect_inputs.len() == 1`, its `primitive`/`request` match the manifest, and (for an aggregate/String response) `binding.schema.is_some()` — i.e. the response binds as a `RealizedHandle` store-handle entry.
- [x] **Step 2: Verify fail** — compile error (`effect_inputs` / `EffectInputBinding` / `bind_effect_inputs` missing).
- [x] **Step 3: Implement.**
  - `EffectInputBinding` next to `ValueInputBinding` (`lowering.rs:92`). Import `EffectId` in the `crate::vir` use.
  - `LoweringArtifact.effect_inputs` field (`lowering.rs:201`).
  - `bind_effect_inputs`: iterate `island.effect_inputs` with `entry = island.value_inputs.len() + k`, read `island.parameters[entry]` and `root.entries.get(entry)` region, compute `schema`/`store_schema`/`payload_element_schema`/`publication_schemas` exactly as `bind_value_inputs` does, carry `primitive`/`request` from the edge.
  - `lower_island`: `let effect_inputs = bind_effect_inputs(island, &contract, &schemas)?;` and set it on the `LoweringArtifact` literal.
  - `with_test_verified_executable`: `effect_inputs: self.effect_inputs.clone()`.
  - Refine the `Op::EffectRequest` guard message in `lower_node` (still a typed diagnostic; effect embedded outside a partitioned test-body value position is not wired in v1).
- [x] **Step 4: Run** `nix shell nixpkgs#cargo-nextest --command cargo nextest run -p vix` → PASS.
- [x] **Step 5: Commit** — `git add -A && git commit --no-verify -m "vix: bind the effect response as a realized value input on the artifact"`

---

### Task 3: Ratchet — thread the request islands through up-front lowering

**Files:**
- Modify: `vix/src/ratchet.rs`
- Test: `vix/tests/primitive_lowering.rs` (append)

**Interfaces:** no new public API; the request islands (`partitioned.effect_islands`) are lowered up front so phase 05 finds them warm, mirroring `wire_islands` (`ratchet.rs:685`). Phase 04 does NOT drive them at evaluate time.

- [x] **Step 1: Failing test** — a prepared run (lowering only, no `execute`) of an effect-bearing source lowers the request island: assert the cache contains the request island's artifact (`cache.lowered(&effect.island).is_some()`), or that `prepare` succeeds and `partitioned.effect_islands` is non-empty and each lowers.
- [x] **Step 2: Verify fail** (if the assertion targets the new up-front lowering path).
- [x] **Step 3: Implement** — in `prepare_run` (`ratchet.rs:679`), after the `wire_islands` loop, add `for effect in &partitioned.effect_islands { cache.get_or_lower(&effect.island)?; }`. (Do NOT add an evaluate-time `effect_lookup`/resolution — that is phase 05.)
- [x] **Step 4: Run** `nix shell nixpkgs#cargo-nextest --command cargo nextest run -p vix` → PASS (effect-free tests: `effect_islands` empty, loop is a no-op).
- [x] **Step 5: Commit** — `git add -A && git commit --no-verify -m "vix: lower effect request islands up front so phase 05 finds them warm"`

---

### Task 4: Phase gate

- [x] Full suite: `nix shell nixpkgs#cargo-nextest --command cargo nextest run -p vix` → all green.
- [x] Clippy: `nix shell nixpkgs#clippy nixpkgs#cargo-nextest --command cargo clippy -p vix --all-targets -- -D warnings` → clean.
- [x] Re-read the diff vs the Global Constraints: `vir` imports no `runtime`; exactly one `EffectEdge` / one `Island.effect_inputs` / one `LoweringArtifact.effect_inputs`; the effect path is a distinct loop/field from wires; no scheduler resolution added; no new `DiagnosticCode`; effect-free islands unchanged.
- [x] Update checkboxes to `[x]`, append landing notes (deviations, the exact effect-edge shape on the artifact, how ratchet carries the request islands, and the precise API phase 05 calls), commit `git add -A && git commit --no-verify -m "vix: mark phase 04 lowering plan complete + landing notes"`, then stop.

## Self-review notes (already applied)

- **Response representation:** the design's "binds as RealizedHandle" describes the *realized value-input channel* (vs the scalar wire channel), which for aggregate/String responses is literally `RealizedHandle` and for records is `InlineComposite`. The effect param binds through the same type-driven path as any value input; the tests use an aggregate/String response so the `RealizedHandle` assertion is literal. Landing notes must flag this for phase 05.
- **Parameter ordering:** value params occupy `parameters[0..V]`, effect params `parameters[V..V+E]`, so the existing `bind_value_inputs` positional zip is byte-for-byte unchanged and `bind_effect_inputs` binds at `entry = V + k`. The effect cut is a second loop after the shared/wire loop precisely to preserve this.
- **`lower_node` guard stays:** after partitioning, a properly-cut effect node is an `Op::Parameter`; the guard only fires for an effect in an un-partitioned position (a callee), which is out of scope for v1. It is a typed diagnostic, not silent miscompile.
- **No scheduler resolution:** the request islands are carried and warmed but never demanded/resolved in phase 04 (`machine.execution.facts-precomputed`); phase 05 reads `LoweringArtifact.effect_inputs` + `PartitionedTest.effect_islands` to resolve at the demand layer.

## Landing notes (phase 04 as landed)

Implemented by an opus subagent; reviewed + gated + landed by the orchestrator.
All four hard constraints held; no deviations on constraints 1–4.

- **What landed, where:**
  - `vir::EffectEdge { primitive: EffectId, request: ValueIslandId }` (vir-local,
    `Copy`) + `Island.effect_inputs: Vec<EffectEdge>` (`vir.rs`, next to
    `wire_inputs`). Partition cut at `Op::EffectRequest` in `Module`'s island
    builder: effect nodes held out of `shared`; each request node becomes its own
    `IslandPurpose::Value` island collected into the new
    `PartitionedTest.effect_islands: Vec<PartitionedValue>`; the consumer node is
    rewritten in place to `Op::Parameter(id)` with the param allocated at
    `parameters[value_inputs.len() + k]`.
  - `lowering::EffectInputBinding { primitive: EffectId, request: ValueIslandId,
    entry, schema, store_schema, payload_element_schema, ty, publication_schemas }`
    + `LoweringArtifact.effect_inputs: Vec<EffectInputBinding>`, produced by
    `bind_effect_inputs` (mirror of `bind_value_inputs`, binds at
    `entry = value_inputs.len() + k`). `lower_node`'s `Op::EffectRequest` guard is
    now an honest v1 limitation, not a not-wired stub.
  - `ratchet.rs`: after the `wire_islands` pre-lower loop, request islands are
    warmed (`for effect in &partitioned.effect_islands { cache.get_or_lower(...) }`).
    No evaluate-time resolution (that is phase 05).

- **Layering (constraint 1) held:** every effect id in `vir`/`compiler`/`lowering`
  is the `vir`-local `EffectId([u8;32])`. `vir` still imports no `runtime`. The
  phase-05 scheduler must convert `EffectId -> runtime::PrimitiveId` at resolution
  (via the manifest / `PrimitiveId::effect_id` inverse).

- **RealizedHandle nuance for phase 05:** the design's "binds as RealizedHandle"
  is the *realized value-input channel* (vs the scalar wire channel). For an
  aggregate/String response that is literally `ValueRepresentation::RealizedHandle`
  (the `String` test asserts `binding.schema.is_some()` — a single `Handle`
  frame word); a *record* response would bind as `InlineComposite`. Phase 05 must
  drive the interned response through the same type-driven value-input path, not
  assume a store handle for every primitive.

- **v1 call-position limitation:** a primitive is callable only from a test-body
  value expression (where partition_test rewrites the node). An `Op::EffectRequest`
  reaching `lower_node` means it sits in an un-partitioned position (e.g. inside a
  callee) — a typed "not supported in v1" diagnostic, not a miscompile. Phase 06's
  first real primitive uses the supported position; broadening is later work.

- **Exact API phase 05 consumes:**
  1. `PartitionedTest.effect_islands: Vec<PartitionedValue>` — the request islands
     (pure), already lowered/warm in the ratchet cache; evaluate one as an ordinary
     demand to get the request `ValueId`.
  2. `LoweringArtifact.effect_inputs: Vec<EffectInputBinding>` on the *consumer*
     island — one per effect edge, carrying `primitive: EffectId`, `request:
     ValueIslandId` (names the effect_islands entry), and `entry` (the value-input
     slot to bind the interned response into, `= value_inputs.len() + k`).
  3. Resolve at the demand layer, intern the response, bind it at `entry`, spawn.

- **Gate note (operational, for phase 05/06):** the full `nextest run -p vix`
  reported `cross_lane_differential::accepted_corpus_agrees_across_native_and_interpreter_lanes`
  as failed — this was a **nextest slow-timeout kill under load (~11)**, not a
  regression. That test genuinely runs ~435s (verified PASS in isolation via
  `cargo test -p vix --test cross_lane_differential`); nextest's configured 300s
  slow window (`.config/nextest.toml`) reaps it when the box is busy. It touches
  the accepted corpus only (no primitives), where phase-04 code is inert
  (`effect_inputs` empty). When gating this stack, run heavy differential/ratchet
  tests on a quiet box or in isolation.

- **Commits (over `vix-prim-03-compiler`):** plan; `partition cuts at
  Op::EffectRequest into request islands + effect edges`; `bind the effect
  response as a realized value input on the artifact`; `pre-lower effect request
  islands on the ratchet seam`; this notes commit.
