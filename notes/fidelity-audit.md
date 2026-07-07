# Fidelity audit — has the cargo/vix build lane strayed from the light?

Read-only audit ordered by Amos, 2026-07-07. No fixes, no refactors. Every
claim below is pinned to file:line in this worktree
(`amos-fidelity-audit`, HEAD `9eb9ce2c7`).

Amos's fear, verbatim: build scripts / proc macros / manifests / resolution
are "all the same job… four different graphs… ours are not actually mixed
because we're not using mutable state like Cargo… it's just a function call
that becomes a node in a graph that becomes evaluated by the machine. I fear
that you guys may have strayed further from the light than I am comfortable
with."

## Verdict in one line

**The fear is largely REFUTED at the architectural level.** Every real process
is a demand-driven graph node behind one seam; the four dependency graphs are
edge-kinds over one pure immutable resolve; build scripts and proc macros are
plain `rustc!`/`build_script!` nodes. There is **no** Rust-side machinery that
bypasses the graph (category iii = empty). What exists is **completeness debt
that is entirely vix-side**: hand-fitted `.vix` programs (name allowlists,
hand-built unit edges, hardcoded cfg ladders) — on the graph, evaluated by the
machine, just incomplete. Every one of those is already flagged, either by the
`monorepo-oracle-recon` note or by the `.vix` code's own pinned gap functions.

| Q | Topic | Verdict |
|---|---|---|
| 1 | Execution paths (rustc/build-script/proc-macro/tool) | **CLEAN** |
| 2 | Four dependency graphs (normal/build/dev/proc-macro-host) | **CLEAN** |
| 3 | Build scripts & proc macros treated as special/hard | **CLEAN** (arch) / **SCAFFOLD-DECLARED** (manifest reader) |
| 4 | Straying census — Rust-side where arch says vix-side | **CLEAN** — no category-(iii) bypass; all (i) or (ii) |

---

## Q1 — Execution paths: every real process is a demanded graph node. CLEAN.

There is exactly **one** process-spawn site in all of `vix/src`:
`vix/src/real_process.rs:218` (`Command::new(&program)`), and it lives inside
the `MachineExecBackend` implementation (`impl MachineExecBackend for
RealProcessBackend`, `real_process.rs:47`). `rg 'Command::new|std::process'
vix/src` returns nothing else. So there is no side-channel that runs rustc/cc/
ar/build-script outside the seam.

The path from a `.vix` `rustc!` to a real process, node by node:

1. **Lowering.** A `rustc!{…}`/`build_script!{…}`/`cc!`/`ar!` command block is
   lowered in `vix/src/machine/lower.rs:6775` (`command_block`). The command
   name is checked against the exec subset `"cc" | "ar" | "rustc" |
   "build_script"` (`lower.rs:6778`); the tool is resolved as a *capability*
   binding (`Rustc::acquire`, `lower.rs:6785`); every argv part is a vix token
   or a spliced vix value (`lower.rs:6788-6791`). The block lowers to a
   `HostCall(EXEC_HOST)` (`EXEC_HOST = 13`, `driver.rs:130`).

2. **Perform (no run yet).** `exec_host` in the driver
   (`driver.rs:3497`) reads the command kind and the vix-provided argv parts
   from the frame (`driver.rs:3499-3529`), then registers a `PendingExecRun`
   and allocs a **tree-exec handle** — a graph node — emitting
   `DriveEvent::RunRequested` (`driver.rs:5804-5831`). The process is *not*
   started here; a handle is returned.

3. **Demand starts it.** The process runs only when a path is demanded from
   that exec node's output tree: `demand_exec_path` → `ensure_run_started` →
   `backend.spawn(request)` (`driver.rs:5891`, `:5834`, `:5862-5886`). With no
   backend set, the same node is served in-process by a `Tool`
   (`schedule_run` → `tool_for` → `FakeRustc`, `driver.rs:5965-5966`,
   `lib.rs:404-412`). This is the load-bearing demand-driven invariant applied
   to execution: nothing is spawned unless its output is transitively demanded.

4. **Two backends, one seam.** Real (`RealProcessTool`, `real_process.rs:183`,
   spawns in a per-run tempdir with `env_clear()`, staging only *declared*
   inputs — `real_process.rs:194,218,250`) and fake (`FakeRustc`,
   `exec.rs:810`, a deterministic content-hash "compiler" that reads only
   declared-input roles via `world.read`, `exec.rs:838-842`). Both implement
   the same `Tool`/`MachineExecBackend` seam and are invoked per-exec-node.
   The fake is a hermetic test double, **not** a parallel bypass.

5. **Command grammar is the observability seam, not synthesis.**
   `assign_roles` (`lib.rs:333`, arms at `:340/:353/:365/:385`) maps each argv
   entry the vix already built to a `Role` (Input/Output/SearchDir/Env/…). It
   assigns roles; it never synthesizes a flag (`lib.rs:399` errors for any
   command outside the subset). This is the `(b)`-ABI observability obligation
   from `where-values-are-declared.md`, realized as an atomic host concern —
   which inputs to demand and which outputs to harvest. Confirmed by the recon
   note independently.

`vix/tests/crate_real_process.rs` drives this end to end against real rustc
(`RealProcessBackend`, `crate_real_process.rs:13`) and compares the machine's
demanded rustc argv to `cargo +nightly build --unit-graph` as an oracle
(`machine_rustc_unit_graph` `:589`, `cargo_unit_graph_oracle` `:649`). The
comparison reads the *actual demand trace* (`DriveEvent::RunRequested … rustc`,
`:546-549`), not a Rust reconstruction of the plan.

**Q1 = CLEAN.** rustc, cc, ar, build scripts and proc-macro builds are all
EXEC_HOST nodes demanded by the machine from the `.vix` program. No Rust-side
special-case machinery bypasses the graph.

---

## Q2 — The four dependency graphs: edge-kinds over one pure resolve. CLEAN.

`rodin/rodin.vix` (1609 lines) is a **pure functional CDCL resolver**. There is
no Rust resolver: `rg 'fn.*resolve|fn.*solve|backtrack|propagate' vix/src`
finds only compile-time name binding (`binder.rs`) and `semver` parsing
(`version.rs`/`version_set.rs`) — the deleted `rodin-core` (RESURRECTION's
"deliberate hole") has no successor in Rust.

- **One immutable index of facts.** `struct Index` carries clauses/guards/gates
  with `guard_kinds`, `gate_kinds`, `gate_targets` (`rodin.vix:179,187` region).
  Dependency edges are *clauses* with a `Gate { kind, target }`.
- **normal / build / dev are gate KINDS, not separate resolves.**
  `gate_active(gate, target) = gate_kind_active(g.kind) &&
  gate_target_active(g.target, target)` (`rodin.vix:655-660`).
  `gate_kind_active` (`:562`) is the consumption gate: `"dev" → false`,
  everything else active — i.e. dev edges don't get consumed transitively,
  matching cargo and the passing fixture
  `transitive_dev_dependency_is_not_consumed`.
- **The proc-macro-host "fourth graph" is the same resolve at a different
  target.** `solve(index, problem, target)` is parameterized by target; the
  host-vs-target split is realized at the unit/build-walk level, where
  proc-macros and build scripts build with `Target::host()`
  (`crate.vix:587` `generic_proc_macro`, `:601` `generic_build_script_compile`,
  `:602`/`:646` `Rustc::acquire(Target::host())`). It is not a separately
  tracked mutable graph.
- **State is threaded immutably (functional), not mutated.** `struct State {
  domains, features, hypotheses, applied, learned }` is rebuilt as a value on
  every step: `Domain { active: true, allowed: old.allowed, selected:
  old.selected }` (`:377,:401`), `..selected` spreads (`:409`), `Step::Pass {
  state, changed }` (`:211`). The `stored_*` helpers (`:243-266`) are
  realization-barrier round-trips (a known vix language awkwardness logged in
  RESURRECTION), not mutation. The hypotheses/learned trail is inherent CDCL,
  not cargo's mutable unit table.

This is exactly the charter: "four different graphs… it's all the same job…
ours are not actually mixed because we're not using mutable state like Cargo."

Fidelity gaps (vix-side, all pre-flagged, none is mutable-state leakage):
`gate_kind_active` only special-cases `"dev"` (`:562`); cfg/target matching is
a hardcoded string-contains ladder (`target_atom_matches` `:577`); feature
resolution + weak-optional activation are distilled to prose but not fully
native (`rodin/PLAN.md`, and the one fixture that needs it is `#[ignore]`d —
recon §3, gap #5/#6).

**Q2 = CLEAN.** One pure `solve` over an immutable `Index`; normal/build/dev
are gate kinds; host/target is a resolve parameter. No cargo-style mutable
unit state anywhere.

---

## Q3 — Build scripts & proc macros: "a function call → a node." CLEAN (arch).

Nowhere is a build script or proc macro treated as special/hard machinery. In
`crate.vix` they are ordinary graph nodes:

- proc-macro = one rustc node, `--crate-type proc-macro`, host target
  (`crate.vix:586-599`, `:763-778`).
- build script = compile it as a bin (`build_script_compile` `:645`), **run**
  it as an exec node (`build_script_run` → `build_script!{…}` `:658-681`), parse
  its stdout directives (`build_directives` via `DOC` host, `:683-685`), feed
  `--cfg`/`OUT_DIR` into the dependent rustc node (`build_script_lib` `:691`).
  That is the whole build-script protocol as a chain of demanded nodes.

The one **real** finding here is the `monorepo-oracle-recon` result, and it is
correctly located: the *manifest reader*, not the exec layer, is fixture-fitted.
`playgrounds/snark/src/bundled/vix/samples/cargo_manifest.vix` stands crate-name
allowlists in for generic key reads:
- `dependency_is_workspace` recognizes only `"blake3"`/`"autocfg"`
  (`cargo_manifest.vix:137-142`);
- `workspace_dependency_path` only `"taxon"` (`:116-121`);
- `dependency_default_features` only `"blake3"`/`"facet-core"`/`"facet-macros"`
  (`:144-155`).

Crucially: **this code is `.vix`, evaluated by the machine — it is on the graph,
just incomplete.** It is not Rust-side machinery. And it is self-declared:
`cargo_manifest.vix` ships its own pinned gap functions
`target_shapes_array_gap()` (`:255`) and `resolved_unit_adaptation_gap()`
(`:269`), and `rodin/PLAN.md` + `notes/monorepo-oracle-recon.md` (on branch
`vixen-facet-recon`) enumerate the rest. The arity-split walk
(`generic_bin_check_with_deps` 0/1/2/dynamic, `crate.vix:407-415`) is a
readability artifact — the `_dynamic_deps` arm (`:465`, via `direct_extern_args`
recursion `:287`) already generalizes N-ary fan-out.

How deep does the allowlist pattern go? **It is fixture scaffolding, not
load-bearing architecture.** The load-bearing pieces — the exec seam (Q1), the
resolve (Q2), the demand-driven walk (`crate.vix` `ResolvedGraph` walker,
`:615-643`) — are generic. What is hardcoded is the *semantic manifest reading*
that feeds them, and the unit-graph *assembly* that connects resolve to walk
(`fixture_resolved_graph`, `crate_real_process.rs:468` — hand-writes edges
`deps:[1]`, `deps:[0,2]` in a **vix test fixture**, gated on a real rodin
solve). Both are on a stated path to generic vix (recon §8 burndown).

**Q3 = CLEAN architecturally; SCAFFOLD-DECLARED for the manifest reader.**

---

## Q4 — Straying census

Classification per the road-2 rule (host fns only for atomic pure functions +
effects; hot loops live in vix).

### (i) Legitimate host fns (atomic pure fn) or effects — sanctioned

- **EXEC seam** — `MachineExecBackend`/`Tool`, `EXEC_HOST` (`driver.rs:108,130`;
  `real_process.rs:47,218`). Effect. The one process-spawn in the crate.
- **Command grammar** `assign_roles` (`lib.rs:333`). Atomic; the observability
  seam; assigns roles, synthesizes nothing.
- **semver host ops** VERSION_PARSE / VERSION_SET_* / VERSION_FIELD
  (`version.rs`, `version_set.rs`, `driver.rs:150,158,159,167`). Atomic pure
  fn; the doctrine's explicitly-sanctioned *measured* hotspot ("VersionSet
  interval throughput"). Note VERSION_FIELD is *also* on the (ii) list below.
- **parse host ops** DOC_PARSE/DOC_GET/DOC_PACKAGE (`driver.rs:138,139,148`) —
  atomic toml reads.
- **effect host ops** FETCH / CRATE_ARCHIVE / GLOB (`driver.rs:135,166,137`) —
  network / archive-extract / fs listing.
- **value ops** STORE_* / MAP_* / ARRAY_* / STRING_* / OPTION_*
  (`driver.rs:117-173`). Atomic; ~24 slated to collapse behind View accessors
  in V10 (optimization, not straying).

### (ii) Declared temporary scaffold with a path to vix

Ranked by blast radius (largest first), all pre-flagged:

1. **`cargo_manifest.vix` name allowlists** (`:117,:137,:144`) — 1356 workspace
   grep-hits of `workspace = true` reduced to a 2-name check. Recon gap #1,
   highest blast radius. vix-side.
2. **No profile-derived rustc flags** — zero `opt-level`/`debug-assertions`/…
   in `vix/src` and no `rustc!` site emits them. Recon gap #3. Absent, not
   wrong; a hole in the `.vix`.
3. **Feature resolution + weak-optional activation** distilled to prose, not
   fully native; the testing fixture is `#[ignore]`d (recon gap #5;
   `rodin/PLAN.md`).
4. **cfg/target gating** = hardcoded string ladder `target_atom_matches`
   (`rodin.vix:577`); single-edge fixtures only. Recon gap #6.
5. **Resolve→unit-graph bridge is hand-built in fixtures only**
   (`fixture_resolved_graph`, `crate_real_process.rs:468`). No production form
   exists. Recon gap #7. **This is the watch-point** (see below).
6. **ELF/OCI/AST_DOC host ops + `call()` special-forms** — RESURRECTION names
   these "the V8 language-level leak… Fable's own debt," scheduled to die in V5
   ("parsers out"). Declared.
7. **VERSION_FIELD intrinsic + string-blob Version** — `rodin/PLAN.md:78`
   explicitly marks it superseded ("Version becomes a vix value").
8. **`stored_*` realization round-trips in rodin** (`rodin.vix:243-266`) — a
   logged language-awkwardness workaround, not architecture.

### (iii) Architectural bypass nobody flagged — Amos's actual fear

**None found.** No Rust code performs dependency resolution, manifest
evaluation, unit-graph planning, or process execution outside the graph. The
resolve is in `rodin.vix`; the manifest read + build walk are in
`cargo_manifest.vix`/`crate.vix`; execution is the single EXEC_HOST seam. The
Rust side holds only atomic pure fns, effects, and the observability command
grammar.

### The one thing to watch (not a bypass today — where a bypass could appear)

The resolve→unit-graph translation exists **only** as hand-authored vix test
fixtures (`fixture_resolved_graph`, `crate_real_process.rs:468`); there is no
production code — Rust *or* vix — that turns a real rodin `SolveResult` into a
`ResolvedGraph` for an arbitrary workspace (recon gap #7). This is a **hole**,
not a bypass. It is called out because it is the single most likely place a
future agent, handed "make the monorepo build," would be tempted to assemble the
unit graph Rust-side for expediency — which *would* be the stray Amos fears. The
faithful move is to grow that assembly in `crate.vix`/`cargo_manifest.vix` as a
demanded computation over the immutable index, exactly as the resolver already
is.

---

## How this maps back to Amos's words

> "It's just a function call that becomes a node in a graph that becomes
> evaluated by the machine."

Confirmed for execution (Q1), resolution (Q2), and build-script/proc-macro
compilation (Q3). `rustc!`/`build_script!` → EXEC_HOST node; `solve` → pure
value over an immutable index; proc-macro/build-script → ordinary rustc/exec
nodes at the host target.

> "Ours are not actually mixed because we're not using mutable state like Cargo."

Confirmed: rodin threads an immutable `State`; normal/build/dev are gate kinds
on one index; no mutable unit table.

> "I fear that you guys may have strayed further from the light than I am
> comfortable with."

The **architecture** has not strayed. The **coverage** has: several `.vix`
programs are fitted to 2–8-package fixtures via name allowlists, hardcoded cfg
ladders, and hand-built unit edges. Every one of those is (a) vix-side, on the
graph, evaluated by the machine, and (b) already flagged — by the recon note or
by the code's own gap functions. None is Rust-side machinery bypassing the
graph. The distance from "the light" is completeness debt, not architectural
drift — and it is a countable burndown list (recon §8), not a vague fog.
