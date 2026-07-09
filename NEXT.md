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
splicing gap discharged by built-in `Arg::{Str,Path,Interpolation}` + lazy pending mounts;
Tree→String text projection landed as `Tree.text(Path)` for single file leaves; no block
expressions in match arms; Doc traversal ergonomics (optional projection without lowering
failure; key enumeration over dependency tables — from manifest ingestion).

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

## HARD GATE from corpus audit (2026-07-07, docs/design/corpus-match-audit-2026-07-07.md in vixen repo)
Audit verdicts: 12 CONSISTENT / 7 DIVERGENT-BENIGN / 5 CORPUS-SILENT / 3 CONTRADICTS-ADJUDICATION.
All three contradictions = rodin learning's fact shape vs the adjudicated design:
- Landed `LearnedNoGood { region, active }` is an IN-SESSION prototype ("local learned-region /
  declared-effect widening") — label it that way; it is NOT R2 observed-read-set learning.
- GATE: before ANY warm/persistent/cross-solve fact work (restart-with-retained-facts is the
  boundary line), facts must grow the adjudicated shape: premise-keyed support traces,
  first-class absence keys, proof/derivation digest, replay-gated install
  (rodin-fact-validity/adjudication.md, rodin-warm-facts-derivations.md). Charter must cite these.
- Sandbox stage 2 charter MUST wire vx-vfsd tracked_observations into ExecOutcome (already noted
  above — audit re-flags it as the thing that keeps stage 1 honest).
- Reloc-walk stays in the artifact-evidence lane (conservative fallback); never conflate with
  observed process read-sets.

## Epoch-closing flags (stage-4 review, both committees, 2026-07-07 — inherit into stage-6 identity freeze)
- ne→le laundering: schema_ref_for → frame_word_for_name round-trips SchemaId via to_ne_bytes,
  payloads write to_le_bytes — correct on all supported (LE) platforms, byte-swaps on BE. Normalize
  schema-word serialization to LE end-to-end BEFORE shared-cache ships. Natural home: stage-2
  container payload rewrite.
- legacy_marker_schema_id: the DefaultHasher half of this flag is STALE — module.rs:1071 already
  uses blake3, domain-separated ("vix-legacy-schema-marker") and length-prefixed; no DefaultHasher
  remains anywhere in vix/src (2026-07-09, read the code). What SURVIVES: it derives a SchemaId
  from the type's RENDERED NAME STRING, not from structure — so `Map<String, Int>` is identified by
  how it was spelled. That is the "SchemaId-bytes-not-name-strings" flag below, and it is the real
  stage-6 item. It covers generic/wrapper frame names that aren't concrete taxon schemas — NOT the
  final canonical encoding.
- Stage-2 containers charter items: retire Sealed-as-empty-list-placeholder (live builtin overloaded
  as don't-care — footgun if anything ever dispatches on empty-list element schema); real typed
  element refs end the empty-list cross-type collapse.

## Mission (queued behind typed-collections fold): exec argv enum + capability paths
Amos-ratified 2026-07-07 (see RESURRECTION "RATIFIED: the two language gaps' designs"):
- Path type: join-only from granted roots (workspace root, crate root, out-dir, node output
  trees); String only as segment; no free cast. Unblocks vix-side ResolvedUnit emission.
- rustc!/exec argv: elements become enum Str | Path | Interpolation-into-node-output-tree
  (subfile references), lazy — demand walks interpolations, creating dep edges + input
  declarations + mount grants. Variable-length computed lists of such elements (n deps →
  n --extern flags) must be expressible. Unblocks --extern splicing.
- Design note: subfile references = per-subfile dependency granularity (rmeta-only consumers
  get early cutoff via argv shape — the RDR decision half).
- SEQUENCING: touches lower.rs exec grammar + driver exec host — wait for typed-collections
  (hash-epoch-containers) to fold first; unit-graph-in-vix agent leaves precise seams/repros.
- REPRESENTATION SKETCH (Amos-endorsed direction, 2026-07-07): exec outputs are Merkle trees
  (ExecTree / DirectoryListingHash(Blake3Hash) already in protocol) → subfiles have independent
  content hashes. Subfile ref = join(tree_root: Path-output-of-node, segments) — type is just
  Path; node provenance rides in the runtime value (capability construction gives this).
  Demand of subfile = demand producer, resolve segments, record read-set entry at SUBFILE hash
  granularity (vfsd File/Directory/LookupMiss already distinguish) → consumer memo keys include
  only what was read → rmeta-unchanged ⇒ rmeta-only consumers memo-hit. Subtlety: execution
  stays per-unit (rustc emits rmeta+rlib in one run); per-subfile is INVALIDATION granularity
  (cutoff, not work-avoidance). rmeta-first pipelining = later refinement, same shape.
- ADDED TO THIS MISSION (build-script agent stop-and-report, 2026-07-07): single-file TEXT
  PROJECTION from a granted tree: file-leaf Tree -> String (loud error on directory), atomic
  pure host over content-addressed bytes, read-observation recorded when tree is an exec
  output. Repro: `run / p"build.stdout"` lowers via tree_project to Tree; no Tree->String
  exists. Blocks the pure-vix cargo: directive parser; the EXISTING Rust-side
  build_directives doc parser is scaffold MARKED FOR RETIREMENT once this lands (fidelity:
  don't extend it). Build-script agent (7908ef2f, branch cargo-directive-protocol) is PARKED
  awaiting this primitive — ping it to resume.

## Fast follow-ups from the JIT investigation (report: origin/jit-lane-profiling, notes/jit-lane-investigation.md)
- The 2× JIT anomaly is RESOLVED-BY-EPOCH (1.06-1.18× on current trunk, was 1.65-2.03×). Confirmed
  trunk: SchemaId-keyed HashMap lookups under std SipHash — 30.8% of JIT active time in ONE
  lookup fn (Amdahl exposure after JIT dispatch removal). FIX: identity/integer hasher for
  SchemaId-keyed maps (SchemaId is already random blake3 bits — SipHash on it is hashing a
  hash). Natural home: blake3 epoch branch or immediately after its fold. Then re-run the bench.
- REGRESSION (new tip): typed .pop() leaves `Tuple<Int, Array<Int>>` schema ref unresolved in
  the LR bench lowering path (suite green — only bites nested tuple-of-typed-array positions).
  Small fix; belongs with the SchemaId-hasher pass.

## Machine bugs on trunk (2026-07-07 evening, both assigned to hash-epoch-blake3 agent)
- TRUNK MEMORY EXPLOSION: 100k accumulator peaks 7GB RSS (~70KB/element) on c3156ed5c —
  predates the carried hasher; suspect typed-collections payload/hashing (converges with the
  4→16 oracle-shard slowdown). Watchdog (memcap 6GB) turns it into a loud single-test FAIL.
- `molten handle -1`: member-only real-workspace Index (145 members) fails at ALL solve rings —
  repro on branch tier-a-scale-measurement @ 02a3d13c9. Fixture-scale never reached this path.
  GATES tier-A (0/863 until fixed). Composition itself works: 145/145 members → Index in 84s
  (interpreted; blake3 lever pending).

## Runner exec service: server half PARKED GREEN (vixenware branch vox-exec-service @ dd8a458e)
Typed vox protocol (req/result, read-sets, tree events, capability advertisement, blake3
newtypes), SandboxedExecutor impl, vx-runner websocket host, real client integration test incl.
typed undeclared-read failure. One test scoped out with note: the fresh-full-lock rodin oracle
(780s+ — it IS the interpreted-solve wall from the run-25 stax profile; perf lane's fixes apply).
NEXT: vix-side client MachineExecBackend speaking this protocol — BLOCKED ON A DIRECTION CALL:
public facet-cc cannot Cargo-dep the private vixenware protocol crate. Options: (a) publish the
protocol crate (it's public-destined anyway), (b) mirror the protocol types in facet-cc until
publication. Amos's call (publication timing).
- DECIDED (Amos): protocol crate lives in the OPEN-SOURCE monorepo (facet-cc); vixenware takes
  the private→public dep. Runner agent executing both halves (facet-cc crate via fresh worktree
  branch exec-protocol-crate → gatekeeper; vixenware rewires to dep on it).
- Amos control questions routed to perf lane: (1) debug-vs-release ring walls (all numbers so
  far were DEBUG nextest builds!); (2) are weavy JIT stencils compiled optimized under debug
  host builds? (build-script flags / profile.dev.package override; measure before/after.)
