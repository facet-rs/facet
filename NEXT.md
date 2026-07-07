# NEXT.md — pre-loaded missions (fire when their gates open)

State as of 2026-07-07 evening: see RESURRECTION.md (local) / private archive branch
`vix-docs-archive` for full context. PR #2463 (`rodin`) carries the day's ~16 folds.
Every mission below is speced to fire with `mcp__paseo__create_agent` (codex/gpt-5.5,
auto-review, worktree off `rodin`) and gate per GATE.md.

## In flight right now (gate these as they land)
- `vix-typed-schemas` (V3 hash epoch: Descriptor<String>→taxon::SchemaRef, containers as declared
  descriptors, blake3 + carried incremental hasher). BIGGEST diff in flight. Committee reviews
  before fold. Demand tripwires + molten differential + rodin fixtures must be green per stage.
- `rodin-tail-loops-learning` (tail-loop the 5 linear interiors + 50-conflict-learning).
- `vix-cargo-manifest` (Cargo.toml workspace ingestion in vix) and `vix-sparse-index`
  (crates.io sparse index → rodin Index rows + req-matching). Demo-1 legs.
- `lr-loop-vix-baseline` (Spike D capstone: LR loop as natural tail recursion — the final
  vix-vs-Rust factor vs the ~50× bar; also whether JIT beats interp post-cache).
- RDR fork study (Osiewicz rust/cargo fork diffs; scout relays).
- vixenware `sandboxed-exec-stage-1` (gate-green, parked; fold target in vixenware = Amos's call).

## Mission: rodin 60-features (feature unification)
FIRE WHEN: rodin-tail-loops-learning folds (same surface: rodin/rodin.vix + fixtures).
The Rust Rodin DID feature unification — the distilled docs specify it: `rodin/docs/60-features.md`
(+ 70-targets-cfg.md). Retired Rust reference readable via
`git -C /Users/amos/vixenware/vixen show 10df3a05^:rodin-core/src/lib.rs` (READ-ONLY; docs are
the spec, cargo is the oracle). Target: implement feature resolution/unification per doc 60;
un-ignore the 5th differential fixture (`cfg_any_and_weak_feature_never_pull_optional_dep`);
add fixtures for: default-features off, feature-activated optional deps, weak features (`pkg?/feat`),
feature unification across the graph. Acceptance: all rodin fixtures green vs real cargo;
corpus + tripwires green; clippy clean.

## Mission: demo-1 integration (the headline — "vix builds facet")
FIRE WHEN: vix-cargo-manifest + vix-sparse-index folded (small targets need no 60-features;
`facet` itself needs 60-features too).
Write the real `cargo.vix` orchestrator: workspace root → manifest ingestion (cargo_manifest.vix)
→ Problem → rodin solve (rodin.vix) → sparse-index rows (index.vix, pinned snapshot fixtures)
→ fetch by cksum (fetch()) → extract (crate_archive()) → generic build walk
(crate.vix ResolvedGraph, landed) → real rustc → run the binary.
LADDER: (1) `taxon` (4 deps, no features drama) → (2) `facet-core` (build.rs) →
(3) `facet` (proc-macro + build script + features → needs 60-features).
Acceptance per rung: builds from clean state, binary/tests run, unit graph matches
`cargo --unit-graph` oracle, lockfile selection matches `cargo generate-lockfile` on the same
pinned index snapshot. De-staticize the rodin→graph bridge as part of this (unit paths/kinds
derived in vix from manifests, not the Rust test adapter).

## Mission: MOLTEN_DROP (the reuse perf unlock)
FIRE WHEN: V3 folds (same surface). Committee round-2 design (see private archive docs 7 +
RESURRECTION): backward last-use analysis in lower.rs inserts MOLTEN_DROP host calls so refs
DEcrement; a value read twice is currently copy-only forever. Risk class: aliasing-corruption
(not eagerness) — gate hard on the corpus-wide VIX_FORCE_MOLTEN_COPY differential + aliasing
tripwires. Also: the "reuse declined at site S" perf diagnostic (observability, not annotation).

## Mission: vix language gaps (talk through with Amos one-by-one first)
Logged by the day's missions: no `sort` primitive (insertion sort hand-written twice now);
aggregates-in-containers hit Realized/Pending/molten barriers (workaround: Int ids + flat row
maps); `Array.pop` surfaces as `Tuple<Int,Array>` (awkward for non-Int); returning `[String]`
unstable; appended fixture code can't call imported std helpers; dynamic `--extern name={Tree}`
splicing gap (from the generic walk); no block expressions in match arms; Doc traversal
ergonomics (optional projection without lowering failure; key enumeration over dependency
tables — from manifest ingestion); no string→Path / path-join bridge (blocks vix-side
ResolvedUnit emission).

## Open investigation: JIT lane slower than interp
Persists AFTER the compile-cache fix (spike D final: JIT ~2× interp at 10k and 100k tokens,
no JitProgram::compile trunk in stax anymore). Something in the JIT lane's host-call/burst
transition costs more than the interpreter's. Needs its own stax dig on a host-call-heavy
workload. Related: spike D's residual profile = Task::run_hosted/Driver::burst (string clone,
memmove, vector growth, hash, allocator) — the stringly part dies with V3; the burst/dispatch
part is the post-V3 perf frontier (LR loop at 293× vs the ~50× bar).

## Later / owned elsewhere
- V10 pull accessors (View API — archive doc 3), V4 host registry (~33 survivors), V5 parsers-
  out-of-keywords (AST→done, JSON=first snark win, TOML follows, ELF→registered capability
  interim, binary-snark = Kaitai-shaped second dialect later).
- RDR: act on the fork-study verdict (rmeta external surgery is NOT shippable — needs rustc-side
  projection; test-selection reloc-walk IS shippable, panic-Location rodata masking needed).
- vixenware stage 2: vfsd prefix mounts + tracked_observations → observed read-sets
  (protocol already shaped for it); runner-as-backend over vox RPC (decision (b), recorded).
- taxon crates.io: placeholder v0.0.0 reserved; publish real vocabulary whenever Amos wants.
