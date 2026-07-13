# Stale-base integration audit — Vix/Weavy tree (2026-07-12)

## 1. Scope and exact base

- **Base (immutable):** `99d8bc34ddca127f92ede5a0a9dd5beeab0e2b42`
  (`docs: require verified Paseo worktree bases`). Confirmed `git rev-parse HEAD`
  equals this SHA and the working tree is clean before any inspection.
- **Method:** read-only. The only repository edit is this artifact under
  `rodin/reviews/`. No implementation code, fixtures, generated files, or specs
  were edited; no push/reset/rebase/revert/checkout/bisect.
- **Subject:** semantic / architectural damage in the authoritative Vix + Weavy
  tree caused by side branches that were substantially behind their integration
  point, across the nine first-parent merges named in the mandate. Each merge's
  actual merge-base, side delta, combined resolution, and trunk correctives were
  reconstructed and compared against the normative `rodin/docs/*` and the
  unchanged canonical rungs (`vix/tests/ratchet/*.vix`).
- **Oracle discipline:** correctness is *not* inferred from a green suite. Where a
  slice is called sound, the certificate is either (a) a normative-doc match read
  against current source, or (b) a named adversarial rung/test that exercises the
  exact path, plus the structural argument for why it cannot silently regress.

### Reconstructed merge geometry (verified)

| Merge | Subject | merge-base | first-parent lag | side commits |
|---|---|---|---|---|
| `1f5c33e884` | coherence lineage | `580e4c3c24` | 6 | 13 |
| `fbebd069ea` | stream selection | `abfdeacc82` | 8 | 8 |
| `5236654f1c` | string operations | `abfdeacc82` | 18 | 10 |
| `8f676cc7bf` | relative paths | `abfdeacc82` | 31 | 4 |
| `f5fe77de81` | captured closures/recursion | `fbebd069ea` | 21 | 5 |
| `9ef6a4e19a` | trace checks/budgets | `86bbc3602f` | 18 | 5 |
| `ab534567e4` | self-tail loops | `a29bd1965b` | 15 | 6 |
| `c9ce9b6398` | Version/VersionSet | `5675e7bffa` | 9 | 15 |
| `f00447decb` | production trace retention | `a184506dd1` | 17 | 4 |

The lags match the mandate. Note the shared base `abfdeacc82` under three merges
and the *chained* bases (`fbebd069ea` is itself the base of `f5fe77de81`;
`86bbc3602f` — a string-status corrective — is the base of `8f676cc7bf`): the
side branches were cut from successive integration points, so a stale side delta
is layered on top of an already-integrated predecessor, which is exactly the
geometry that hides duplicated authorities.

## 2. Behavioral regression coverage vs architectural certification

These are different guarantees and the tree conflates them in one place worth
naming.

- **Behavioral regression coverage (strong).** Every merged feature has an
  adversarial rung and/or Rust certificate that pins concrete value/check
  identities: version precedence & sets (`vix/tests/version_lane.rs`,
  `structural_order.rs`), join-only paths (`vix/tests/ratchet/046-paths.vix`,
  `047-string-to-path.reject.vix`), string ops (rung 045 + weavy `verified.rs`
  typing), tail loops (`ratchet_runner.rs:5601`, `:5647`; rung 050), trace
  budgets (`vix/tests/rung050_trace_budget.rs`), generator failure payloads
  (`faulting_scrutinee`, `ratchet_runner.rs:1433`).

- **Architectural certification (partial, by construction + matrix).** The Weavy
  substrate runs two lanes with **separately authored op authorities**:
  interpreter (`weavy/src/task.rs`, `verified.rs`) and native copy-and-patch
  (`weavy/stencils/task_ops.rs` + `weavy/src/jit/task_lane.rs`). Two structural
  facts bound the divergence risk:
  1. **No silent per-op fallback.** `Executable::with_trace_mode`
     (`weavy/src/exec.rs:893-926`) selects Native iff `available()` and not
     env-disabled; otherwise Interpreter — a whole-program choice, never per-op.
     The native op dispatch (`weavy/src/jit/task_lane.rs:410-660`) is an
     **exhaustive `match op` with no `_` arm**, so a new op without a stencil
     fails to compile rather than silently degrading. `native_available` /
     `native_compiled` / `fallback` are surfaced as typed facts
     (`weavy/src/exec.rs:39-43`, `vix/src/runtime/observe.rs:48-61`), not hidden.
  2. **Both lanes are exercised over the corpus across the CI OS matrix.** Native
     targets are exactly `(macos,aarch64)` and `(linux,x86_64)`
     (`weavy/build/jit_config.rs:47-50`). The CI matrix (`.github/workflows/test.yml`)
     runs `--workspace` on `bearcove-ubuntu-24.04` (linux-x86_64 → **native**),
     `macos-26` (aarch64 → **native**), `ubuntu-24.04-arm` (linux-aarch64 →
     **interpreter**) and `windows-2025` (x86_64 → **interpreter**). So the vix
     ratchet corpus runs on both lanes, and the corpus is authored to be
     lane-portable — 10 sites in `ratchet_runner.rs` guard native-only counter
     assertions behind `WEAVY_JIT != "0"`.

The **residual** gap is the *shape* of the equivalence guarantee, not its
existence — see finding F1. This is the one place where "the suite is green" must
not be read as "the two lanes are certified identical."

## 3. Per-merge disposition

All nine are **sound/current** or **corrected-after-merge**. No concrete defect
(proven wrong output on a reachable input) survives on the current tree.

| Merge | Disposition | Certificate / correction |
|---|---|---|
| `1f5c33e884` coherence | corrected-after-merge | Correctives `e59d64009` + `e5f6bcd4b` are **benign large-error-variant boxing** (`FailureValue` boxed in `GeneratorOutcome::LanguageFailure` / `RunError`), not semantic repair. Payload semantics intact and pinned by `faulting_scrutinee` (`ratchet_runner.rs:1433` asserts the scrutinee's `IndexOutOfBounds` survives as a typed failure). Sound. |
| `fbebd069ea` stream-sel | sound/current | Only a docs follow-up (`67f26da5c`); no code corrective was needed. Stream selection lowering is matrix-covered on both lanes. |
| `5236654f1c` string-ops | corrected-after-merge | `db641ed15` (scheduler: string outcomes threaded after generators) + `86bbc3602` (verifier `require_scalar_write` → `require_status_write` for the two byte-comparable string ops, `verified.rs:2663`,`:2676`). The status-typing fix is **lane-independent** (it is a verify-time contract, not an executor path), so it protects both lanes. Duplicated authorities present; see F1. |
| `8f676cc7bf` rel-paths | sound (corrected) | Path type is **join-only** with a reject certificate: rung 046 constructs via `/` only; rung 047 rejects `let p: Path = s` (String→Path) at the declared site. Correctives `90c8bd536` (test) + `e5f6bcd4b` (boxing). Capability-grant tracking is a *documented future mission* (`NEXT.md:110-138`), not a regression. Highest lag (31) but structurally clean. |
| `f5fe77de81` closures | corrected-after-merge | `5afb0a749` restored closure/path token boundaries in the surface grammar + lowering; `a29bd1965` simplified the capture check. Rungs 048 (capture) / 049 (recursion) / 052 (higher-order) pin behavior. |
| `9ef6a4e19a` trace-budget | corrected-after-merge | `60863a83b` (reject late budget completions cleanly, `budget.rs`) + `c98421f05` (enforce source-declared test budgets). `rung050_trace_budget.rs` is the adversarial certificate. |
| `ab534567e4` self-tail | corrected/sound | Native + innards tail-loop certificates (`ratchet_runner.rs:5601`, `:5647`) prove Production strips instrumentation to zero instructions while Innards retains marks; rung 050 deep-tail-recursion. Corrective `ff9e0fdbb` composes rung 050 with declared budgets. |
| `c9ce9b6398` version | sound/current | `vix/std/version.vix` matches doc 30 exactly: caret boundary `caret_upper` (`:200-208`) = next-major/minor/patch per compat class; prerelease admission `admission_for`/`prerelease_allowed` (`:261-287`) admits a prerelease only when the bound is itself a prerelease on the same release line; precedence (`:76-90`) ignores build metadata. Pinned by `version_lane.rs` / `structural_order.rs`. |
| `f00447decb` trace-retention | sound/current | The merge adds `ExecutionPhase` observation hooks + child-process RSS sampling (`ratchet.rs` `execute_with_observer`, `run_lane` `observe(...)`). Observation is **strictly out-of-band**: trace checks read the *frozen counter snapshot* (`ratchet.rs:581`), never retained events, and the `observe` closure has no return into runtime state. Bounding retention cannot change a value identity. Traces do **not** create observed behavior. |

## 4. Ranked findings

No finding is a concrete defect; both are architectural certification concerns.
Ranked by severity.

### F1 — Interpreter/native equivalence is certified only at declared-assertion granularity, with no single differential harness over the corpus  (severity: medium)

Every op the nine merges added exists as **two independently authored bodies** —
interpreter (`weavy/src/task.rs`, dispatched via `verified.rs`) and native
stencil (`weavy/stencils/task_ops.rs` + `weavy/src/jit/task_lane.rs`). Equivalence
is inferred from *each lane independently passing the same declared rung
assertions on different OS jobs* (§2), not from any harness that runs one input
through both lanes and asserts identical value identities over the corpus. The
only in-process differential comparisons are hand-authored per-scenario in weavy
unit tests (`weavy/src/exec.rs:3295-3326`, `run_public_ordered_write` and
siblings), which cover a handful of ordered-map/env cases — not the 150+ rung
corpus and not the string/path/version/fold ops.

- **Failure scenario:** a stale-origin stencil (e.g. the string-status typing bug
  that `86bbc3602` fixed, had it landed in the *executor* rather than the shared
  verifier) computes a value in a dimension a rung's `expect_eq` does not pin.
  Both lanes pass their assertions on their OS jobs; the divergence is invisible.
  The risk is exactly proportional to assertion completeness of each rung.
- **Smallest corrective certificate:** an in-process corpus sweep that, on a
  native target, compiles each rung once and executes it through both a native
  `Executable` and a `WEAVY_JIT=0`-forced interpreter `Executable`, asserting
  equal check/value *family identities* (the `RatchetReport::agrees()` shape at
  `ratchet.rs:482`, but cross-lane instead of cross-chaos). The primitive already
  exists — the `force_interpreter` env toggle (`weavy/src/exec.rs:1582`,
  `2618-2629`); this finding is a missing *sweep*, not a missing capability.

### F2 — Interpreter-lane corpus coverage is incidental to the CI OS matrix, not a pinned leg  (severity: low)

The interpreter lane runs the corpus in CI *only because* `windows-2025` and
`ubuntu-24.04-arm` happen not to be native copy-patch targets
(`weavy/build/jit_config.rs:47-50`). There is no named `WEAVY_JIT=0` job. The
design intent is asserted in a source comment (`ratchet_runner.rs:4102-4103`,
"both lanes produce identical checks … never a per-program lane fallback") but is
not enforced by a leg that would survive a matrix edit.

- **Failure scenario:** the Windows and linux-aarch64 runners are dropped or
  their filters narrowed (a plausible CI-cost edit); interpreter-lane corpus
  coverage silently vanishes and F1's divergence class becomes wholly uncaught,
  with nothing red to signal it.
- **Smallest corrective certificate:** one pinned CI leg on a *native* target
  (`bearcove-ubuntu-24.04` or `macos-26`) that runs `package(vix)` (at minimum
  the ratchet binary + `accepted_rungs_verify_and_execute_through_one_executable`)
  with `WEAVY_JIT=0`. The nextest override for that certificate already documents
  a ~134s interpreter-lane run (`.config/nextest.toml`), so the cost is known.

## 5. Corrective certificate / code seam for every unresolved risk

| Risk | Smallest seam |
|---|---|
| F1 lane divergence in unpinned output dimensions | Corpus-wide cross-lane differential test reusing `force_interpreter` (`weavy/src/exec.rs:1582`) + `RatchetReport` family identities (`vix/src/ratchet.rs:482`). |
| F2 incidental interpreter coverage | One pinned `WEAVY_JIT=0` `package(vix)` CI leg on a native target. |

Everything else in the nine merges is closed: the correctives listed in §3 are
present in current source (e.g. the fold-seed empty-array fallback restored by
`7ee1da1e3` is live at `vix/src/compiler.rs:4578-4592`, and `require_status_write`
is live at `weavy/src/verified.rs`).

## 6. Prior stale diagnoses to withdraw

1. **My own in-audit hypothesis — "the interpreter lane is never exercised over
   the corpus in CI" — is WITHDRAWN.** It was formed after confirming no CI job
   sets `WEAVY_JIT=0`, but `weavy/build/jit_config.rs:47-50` restricts native
   copy-patch to `(macos,aarch64)`/`(linux,x86_64)`, so the `windows-2025` and
   `ubuntu-24.04-arm` legs already run the corpus on the interpreter lane. Both
   lanes are covered. The accurate residual concern is the *shape* of the
   guarantee (F1) and its *incidental* wiring (F2), not absence of coverage.

2. **`NEXT.md` epoch flag `legacy_marker_schema_id` (DefaultHasher half) —
   confirmed STALE, withdraw.** `vix/src/module.rs:1071` (`legacy_marker_schema_id`)
   uses `blake3`, domain-separated `"vix-legacy-schema-marker"` and
   length-prefixed (LE); a crate-wide grep finds no `DefaultHasher` in `vix/src`.
   NEXT.md already annotates this half as stale; this audit confirms it against
   current source. (The surviving "SchemaId-from-rendered-name-string" concern is
   a *separate* stage-6 item and is out of scope for these merges.)

3. **`NEXT.md` epoch flag "ne→le laundering" — resolved, withdraw as a live
   risk.** No `to_ne_bytes` remains anywhere in `vix/src`; schema words now round-
   trip little-endian end-to-end (`module.rs:216`, `:220`, `:1077`, `:1086`). The
   BE byte-swap hazard the flag described is closed. (Re-flag only if a future
   change reintroduces `to_ne_bytes` on the schema-word path.)

## 7. What was NOT found (explicit negatives)

Searched for and did not find, on the current tree: duplicated *resolver*
authorities (rodin.vix was untouched by these merges); obsolete fallback paths
re-armed by a stale delta (the only fallback is the whole-program lane choice,
typed and observable); stale ABI/type assumptions in the schema-word path (LE
end-to-end); host/raw-evaluator substitution or test-only seams bypassing the
production path (`execute_with_observer` is explicitly a non-bypassing seam,
`ratchet.rs:616-620`); incorrect identity/hash boundaries in the marker path
(blake3, length-prefixed); traces that create observed behavior (trace checks
read frozen counters). The generator-failure and fold-seed regressions that stale
refactors *did* introduce were caught and corrected on trunk, and the corrections
are complete in current source.

## 8. Resolution (2026-07-12, both findings closed)

Both ranked findings are now closed by corrective certificates on this branch,
built forward from the audit merge `c9819bbf8`.

**F1 — cross-lane equivalence granularity.** The `force_interpreter` seam named
in §4/§5 was environment-only (`WEAVY_JIT`, `weavy/src/exec.rs:1582`), which
races parallel nextest, so instead of a shared toggle a *typed per-executable*
lane seam was added: `weavy::exec::LaneRequest { Auto, Interpreter, Native }` +
`Executable::with_lane` (`with_trace_mode` now delegates to `with_lane(Auto)`,
preserving the exact environment-driven production policy; a forced interpreter
reports the new `FallbackReason::DisabledByRequest`). Vix threads the request
per-executable through `LoweringCache::for_lane` → `lower_island`, and exposes
the production-shaped path as `ratchet::{run_source_with_lane,
prepare_source_with_lane}`. The certificate
`vix/tests/cross_lane_differential.rs`
(`accepted_corpus_agrees_across_native_and_interpreter_lanes`) sweeps the
accepted canonical corpus through rung 065 (respecting reject semantics): on a
native-capable host each accepted rung compiles once and executes through an
explicitly-selected native `Executable` and an explicitly-selected interpreter
`Executable`, and the full provenance-keyed check/failure family
(`SuiteRun::check_family` — carrying value identities, demand-argument
identities, failure values, and failure-context attribution), the published
`value_family`, the completion facts, and the lane-invariant counters are
asserted identical across *both* the plain and chaos suites. The two
lane-attribution spawn counters are the documented semantic-boundary exclusion,
and are used positively to prove lane purity so a silent native→interpreter
fallback cannot manufacture a false-green. On non-native targets it skips through
`weavy::jit::task_lane::available()`, the same rule the other in-tree cross-lane
tests use. Green on the native lane (all accepted rungs differentiated; reject
semantics certified). Commits: red `a8f4a3311`, green `f6a3eff76`.

**F2 — incidental interpreter coverage.** A pinned CI leg `vix-interpreter-lane`
(`.github/workflows/test.yml`) runs on `bearcove-ubuntu-24.04` — a native
copy-patch target — with `WEAVY_JIT=0`. Because `WEAVY_JIT` is a build-time cfg
gate, the whole build is interpreter-only, so the canonical corpus runner
(`accepted_rungs_verify_and_execute_through_one_executable` and the rest of
`binary(ratchet_runner)`) executes on the interpreter even on a box that would
otherwise select native — pinning interpreter coverage against an OS-matrix edit
that drops the windows / linux-aarch64 legs. The selection is bounded and exact
(`package(vix) & (binary(ratchet_runner) | binary(cross_lane_differential))`)
and reuses the existing container/node/rust-1.92/rust-cache conventions.
Verified after integrating canonical snapshot rungs 060–061:
`WEAVY_JIT=0` nextest of the exact filter is 108/108 pass; `actionlint` clean.
Commit: `9443ac2f0`.

Verification (local, macOS aarch64 = native): the oracle-backed snapshot plus
cross-lane filter is 11/11 pass (the corpus certificate genuinely drives both
authorities through rung 061); the full Vix+Weavy suite is 694/694 pass;
`WEAVY_JIT=0` on the pinned CI filter is 108/108 pass (the cross-lane cert takes
its non-native skip); `cargo check --workspace --all-features --all-targets` is
clean; strict workspace clippy with `-D warnings` is clean; and
`cargo fmt --all --check` is clean.
The `.config/nextest.toml` leash for the differential is `300s × 4` because it
drives rungs 050/051 through the interpreter twice (~390s observed under full
`--workspace` contention). Residual boundary: none beyond the *documented*
exclusion of the two lane-attribution spawn counters — every other semantic
dimension of the run is compared. (Follow-up commits `305219c64` rustfmt,
`ab6da45e9` leash.)

## Evidence / commands used

- `git rev-parse HEAD` → base confirmed; `git status --porcelain` → clean.
- `git show -s --format` + `git merge-base` + `git rev-list --count` per merge →
  §1 geometry.
- `git diff <merge>^1 <merge> -- <path>`, `git show <corrective>` → §3 correctives.
- Source reads: `rodin/docs/{00,10,20,30,40,50,60,70,90,05}*.md`,
  `vix/std/version.vix`, `vix/src/{ratchet,module,compiler}.rs`,
  `vix/src/runtime/{observe,scheduler}.rs`, `weavy/src/exec.rs`,
  `weavy/src/jit/task_lane.rs`, `weavy/build/jit_config.rs`,
  `.github/workflows/test.yml`, `.config/nextest.toml`,
  `vix/tests/ratchet/{046,047}*.vix`, `vix/tests/ratchet_runner.rs`.
