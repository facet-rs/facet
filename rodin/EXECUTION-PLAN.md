# Rodin critical-path execution plan

This is the active implementation plan for reaching a native Rodin without
repeating the workaround-driven port in `rodin/rodin.vix` or
`vix/corpus-next/rodin.vix`.

The representation-neutral solver doctrine remains in `rodin/docs/`. Cargo is
the only behavioral oracle. This file controls implementation order, progress
reporting, gates, and stop conditions.

## Outcome

Build a production Rodin as a pure Vix computation:

```text
PackageUniverse + Roots + FeatureRequests + Target + Policy
    -> SolveResult | typed Conflict
```

The implementation is derived from `rodin/docs/`, not ported from either old
Vix solver. It preserves the researched strategy:

- normalized `VersionSet` domain narrowing;
- propagation to a fixpoint;
- deterministic package and version selection;
- persistent branch state;
- chronological backtracking;
- region-based no-good learning, widening, subsumption, and unit propagation;
- machine-owned content identity, memoization, receipts, and incremental reuse.

Sparse-registry ingestion, manifest decoding, diagnostics, and text rendering
are adapters around that kernel. They do not land before the kernel passes real
Cargo fixtures.

## Current checkpoint

At the time of the latest authoritative integration checkpoint
(`60863a83b`):

- the canonical rung prefix is green through rung 049. The integrated full Vix
  run `dc0b8068-858a-4fc7-bce3-9fbad03a020c` passed 339/339 tests, and the
  integrated full Weavy release run
  `8fce57b6-18d8-41e2-a206-2f1ef4f36a34` passed 228/228. Workspace all-target
  check, strict all-feature/all-target Clippy, formatting, and diff checks are
  green at the checkpoint;
- rung 031 executes unchanged through the completed two-stage generator path.
  One verified generator task runs real `Match`/`If` control and publishes only
  taken `YieldSiteId`s; the runner then evaluates those provenance-keyed Value
  checks as ordinary pure demands. Untaken arms publish nothing, plain/chaos
  agreement compares the provenance family rather than append order, and the
  zero-dynamic-key base case transports no capture, handle, or content hash;
- rungs 033 through 040 execute position-keyed collection, key-preserving
  filtering, `filter_map`, composed-key `flat_map`, canonical structural
  sorting, deterministic folding, structural stream selection/decomposition,
  ordered-Map equality, and caller-supplied `Order<T>` through verified
  execution;
- `Array.split_last()` and `Stream.split_min()` return ordinary immutable
  decomposition values. `split_min` realizes its remainder as `[V]` in key
  order, omits exactly the selected row, and preserves equal duplicates at
  distinct keys;
- Weavy owns the verified persistent ordered Map/Set arena, including probe,
  insert, union, iteration, and interpreter/JIT parity; Vix rungs 041-044 run
  through it with typed `MissingKey` and `DuplicateKey` outcomes;
- `Map.values()` projects values in canonical key order and
  `Array.sorted()` preserves duplicates while ordering Int/String/aggregate
  leaves by structural semantics;
- checked String primitives (rung 045) now use closed typed status outcomes and
  preserve interpreter/native faults for missing delimiters, invalid and
  overflowing integers, and unresident operands. Relative Path construction,
  empty-root joining, byte projection to String, and implicit String-to-Path
  rejection (046-047) are folded through verified operations rather than host
  conversions;
- captured closures (048) source captured values from their own exact declared
  regions rather than structural closure subwords, including direct and
  `Array.map` calls. Plain non-tail recursion (049) retains the verified call
  frame ABI. Focused default and `WEAVY_JIT=0` certificates are green for both;
- `CheckRecipe` now distinguishes demanded Value checks from post-run Trace
  checks. Scheduler/memo/store bounds are evaluated against one frozen snapshot
  after all selected Value checks, with no island, demand, memo entry, or intern
  for the Trace check itself. Typed wall/RSS metadata is enforced by an outer
  child-process watchdog that kills runaway native work; default run
  `4933603b-4592-4b12-abd9-f18880f24a55` and interpreter run
  `1be4cb14-7092-4da7-86e6-1358bc983144` each passed 15/15 focused checks;
- rung 050 is the first canonical red boundary. Its preserved red checkpoint
  is `8b618a7c4`. The canonical source now parses and its TraceCheck/budget
  substrate is live; the remaining boundary is verifier-visible self-tail-loop
  lowering with a cheap interior pollpoint. Ordinary recursion remains the
  rung-049 call-frame path;
- the persistent AVL core has a 200k insertion scaling oracle, but neither the
  rung-051 array certificate nor the end-to-end rung-138 Map proof is yet
  established. Range/fold driving, shared-demand extraction,
  molten-to-store publication, non-colliding live/frozen handles, the identity
  epoch's closed framed writer, and production arena observability remain
  explicit seams;
- the Cargo fixture harness exists in `vix/tests/rodin_fixtures.rs` and its Cargo
  side is independently runnable;
- rungs 098 and 100 still use a recorded `expected_selection()` from the deleted
  reference resolver. That contradicts the Cargo-only oracle doctrine and must
  be adjudicated before those rungs are claimed.

This section is a snapshot, not a durable source of truth. Each implementation
turn starts by checking the branch, preserved checkpoints, and focused gates.

## Two progress measures

The existing ratchet is not renumbered or reordered.

### Canonical score

The canonical score remains the highest consecutive green rung. Every rule in
`vix/tests/ratchet/README.md` and `FOUNDATION.md` still applies. A green rung
above a red rung does not change this score.

### Rodin readiness

A second report tracks a named set of existing rungs in implementation-priority
order. It does not create alternate semantics, duplicate rung sources, or relax
the foundation contract.

The runner must eventually report both facts, for example:

```text
canonical-prefix: 026
rodin-readiness: collections 7/12, solver 0/18, scale 1/5
```

Rules for the priority track:

1. A track entry references the original rung file; no copied or weakened test
   is allowed.
2. Original rung numbers and surface-introduction rules remain authoritative.
3. A selected rung may become green above the canonical red boundary, but it
   does not increase canonical score.
4. If a selected rung needs an unselected prerequisite, add that prerequisite
   to the track or implement the canonical dependency. Never encode a local
   workaround in the selected rung.
5. Reject and warning rungs retain their declared diagnostic behavior.
6. Design errors in a rung are resolved explicitly by Amos and committed as
   specification changes; implementing agents do not silently edit them.

## Rodin priority track

The track is grouped by what it proves, not by numerical adjacency.

### R0 — accepted foundation and fold recovery

- `001-026`: retain green through the verified production path.
- `027`: audit and fold the preserved Array.map checkpoint instead of
  reimplementing it.

Exit evidence:

- accepted rungs `001-027` pass through `VerifiedProgram` and `Executable` in
  plain and chaos lanes;
- no legacy evaluator or raw Weavy execution path is used;
- full workspace check, strict Clippy, formatting, and diff checks pass.

### R1 — value-semantic collections

Canonical collection vocabulary and the transforms used by a solver:

- `028-045`: array streams, fold, predicates, decomposition, filtering,
  explicit order, Map, Set, and string parsing;
- `048-059`: closures, recursion, tail loops, higher-order functions, demand
  selectivity, and memo identity;
- `141-146`: addressed Map reads, duplicate-key failures, parse failure,
  `must_use`, and rejection of mutation-shaped names.

Pulled-forward quality gates:

- `123`: molten and forced-copy results are identical;
- `138`: 200k Map additions do not copy or intern per row;
- `140`: memo lookup remains allocation-free at scale.

Required Map/Set semantics:

- `map + (key, value)` adds a provably new row and fails with typed
  `DuplicateKey` on collision;
- `left ++ right` combines disjoint collections and fails on overlapping Map
  keys;
- `map.with (key, value)` explicitly inserts or replaces;
- `map.get(key)` produces `V` or an addressed typed `MissingKey` failure;
- `map.has(key)` is membership-only and never demands the value;
- Map keys and Set elements observe structural content order, never hash or
  insertion order;
- Set construction and union deduplicate by semantic equality;
- one-item accumulation may be molten under the as-if law but publishes once;
- interpreter and JIT use the same verified ordered-node arena and fault model.

Dynamic test codata is an R1 dependency, not a harness-only workaround. Rung
031 decides at runtime whether its match publishes three checks or one. The
faithful shape is:

1. `Array.split_last` remains an ordinary pure
   `[T] -> Option<(T, [T])>` operation and lands independently.
2. A test body lowers to one verified generator task. Taken control-flow arms
   append descriptors containing a stable yield-site identity plus any stable
   keys contributed by keyed dynamic iteration. A delivery ordinal, captured
   result, task-local handle, or evaluated `Check` is never a stream key.
3. Draining constructs the provenance-keyed family of `Check` descriptors and
   demands no check operand. Each selected Value check is evaluated afterward
   as an ordinary pure demand; its operands remain graph wires and are demanded
   only by that evaluation. Untaken arms publish nothing, so there are no
   phantom checks.
4. Rung 031 is the zero-dynamic-key base case: its descriptor is just the
   `YieldSiteId`, and the existing self-contained check island re-demands pure
   projections through the ordinary memo path. Later keyed codata extends the
   same descriptor with dynamic provenance keys rather than captured values.
5. Stream-element identity (yield provenance) and evaluation memo identity
   (`DemandKey`) remain distinct. Equal check values at distinct provenance
   keys are distinct stream elements even when their evaluation work dedupes.
6. Chaos replay must reproduce the same provenance-to-outcome family;
   publication arrival order is not semantic. Pure check islands retain their
   existing prohibition on yielding.

This reuses the existing memo/evaluation machinery. It does not evaluate checks
inside the generator, eagerly freeze operands, add a host observer, suspend for
mid-drive interning, or constant-fold conditional yields. Weavy owns interior
molten construction; the Vix scheduler remains the only edge-publication and
identity authority when later dynamic key values cross an island boundary.

Rungs 050 and 051 are one scale-substrate sequence, not two local optimizer
tests. They land in this dependency order:

1. Check construction distinguishes `ValueCheck` from `TraceCheck`. Value
   checks remain ordinary demanded islands; scheduler/memo/store assertions are
   deferred until the run is complete and inspect a frozen counter/event
   snapshot without demanding their described operands or counting their own
   reporting work.
2. `#[test { budget_wall, budget_rss }]` is parsed into typed test metadata and
   enforced by an outer runner that can terminate an over-budget execution.
   An in-process elapsed-time assertion that cannot stop a stuck native loop is
   not enforcement; an inert parsed attribute is not a gate.
3. A self-tail call lowers to a verifier-visible in-frame loop with an interior
   pollpoint. It copies the next argument set without overlap, touches no
   scheduler/memo/identity machinery at the backedge, and has interpreter/JIT
   parity. Ordinary non-tail recursion keeps the verified call-frame path.
4. `range where { from, to } -> [Int]` builds the specified dense value without
   a scheduler request per element. `Array.fold` may select a proven-strict
   in-frame execution shape, but the forced-copy differential remains able to
   select the non-molten shape.
5. The molten array accumulator is a verifier-confined builder, not a mutable
   public Array handle. Builder creation/push/finish are non-copyable interior
   operations; the verifier prevents escape and the finished immutable Array
   is observationally identical to repeated by-value `+`.
6. The rung-051 cost model extracts a value node when it has at least two
   ValueCheck consumers and its representation is an aggregate
   `RealizedHandle` (Array, Map, or Set). Cheap inline scalars remain inside the
   check recipes, and source-level `let` syntax is not an extraction or identity
   boundary. This is a partitioning heuristic that may grow with measured
   economics, not a permanent language distinction. The extracted aggregate
   demand crosses the island edge once through scheduler-owned framed
   publication, and each check consumes the same published `ValueId`. Rung 051
   must therefore witness one million-element construction once, not four fast
   recomputations.
7. The production certificate measures the inactive/active molten choice,
   store publication count, memo entries, scheduler contacts, wall time, and
   peak RSS together. Passing only the value assertions or only a core arena
   microbenchmark does not satisfy the rung.

Rung 051 lands through these forward checkpoints:

1. The Vix runtime adopts one explicit identity epoch: a closed role-tagged
   framed writer plus an owned, pre-resolved semantic tree and
   `Store::intern_tree`. Stable Vix `SchemaId`s come from canonical `Type`
   encoding; program-local Weavy schema ordinals, ABI offsets, padding, and
   handle integers never enter identity. The new digest is not claimed to be
   bit-compatible with the retiring flat/raw encoding.
2. `range where { from, to }` allocates one dense array and fills it in-frame.
   Range and fold loop bodies contain the same cheap interior-pollpoint
   vocabulary as rung 050 and emit no per-iteration trace marks, scheduler
   contacts, store operations, or identities.
3. A completed value-island task exposes its molten result only through a
   borrow-scoped opaque resolver. Vix may walk typed payload bytes while the
   `ExecTask` lives; no task-local handle integer or `FrozenRef` escapes that
   borrow, and Weavy computes no semantic identity.
4. The molten fold shape is admitted only for the exact strict one-item-append
   closure: the accumulator is consumed once as the append base, does not
   otherwise escape, and the appended expression is evaluated exactly once.
   Arbitrary folds retain the semantic copy path; forced-copy differential
   coverage uses a bounded input rather than the million-element rung.
5. Shared-value extraction is gated by an explicit publication-capability
   registry. The initial eligible representation is dense Array. A qualifying
   shared Map or Set is a typed red diagnostic until ordered freeze exists; the
   partitioner never selects an aggregate it cannot publish.
6. A value island is nominated by the content-free location
   `test/<test>/value/<stable-id>`, where the stable id comes from canonical
   graph provenance rather than partition-vector or arrival order. Its
   `DemandKey` remains recipe plus arguments and uses ordinary within-runtime
   memo reuse; there is no private cross-test cache.
7. Scheduler-owned `realize_value` walks the opaque task result into the owned
   semantic tree, interns bottom-up once, and binds consumers with the resulting
   store handle and `ValueId`. A failed shared demand propagates on each
   consumer edge with that consumer's rebuilt report context; no unevaluated
   check is assigned a fabricated result identity.
8. Production counters distinguish the value-island spawn from total check
   spawns and record one aggregate freeze per successful lane, active versus
   forced-copy fold selection, bytes hashed, and peak molten bytes/nodes. The
   wall/RSS watchdog and TraceCheck substrate then assert those facts in the
   unchanged canonical rung.

Before any composite dynamic key or completed aggregate crosses that boundary,
the runtime Store must intern it through the canonical framed value walk:
embedded handles contribute their referents' content identities, never their
process-local integer values. The retiring machine driver's recursive
descriptor walk is the migration source for traversal and handle resolution,
not for encoding: its direct `hasher.update` format must be replaced by a
closed framed-writer API (`start`, `field`, `variant`, `seq-len`, `map-pair`,
and `bytes-len`). Raw realized-byte hashing is valid only for contracts whose
identity shape is entirely scalar/opaque; a generic Weavy serializer or second
identity authority is forbidden.

The rung-138 scale certificate is production-shaped only when all of these are
measured together:

- one in-frame loop carries a live ordered root through the accumulation;
- the completed Map freezes and interns once when it crosses the island edge;
- wall/RSS budgets and `store_interns_at_most`/`memo_entries_at_most` are
  enforced by the runner rather than parsed as inert syntax;
- production counters expose ordered arena growth and reuse, so the proof does
  not infer cost from a small core benchmark;
- live molten roots and frozen store handles occupy disjoint encodings.

Forbidden implementations:

- dense-array copy-on-every-insert;
- intern-on-every-insert;
- interpreter-only ordered operations;
- unverified comparator callbacks;
- handle-integer comparison for semantic equality or order;
- a second Map/Set representation hidden in Vix.

### R2 — solver values and the miniature solver

- `060-061`: canonical structural result rendering needed by solution evidence;
- `083`: full SemVer parsing and precedence as Vix values;
- `084`: canonical, prerelease-aware `VersionSet` algebra;
- `085`: typed, lazy package rows;
- `086-088`: persistent domains, narrowing, and typed conflict values;
- `089-093`: solve, backtrack, exhaust, learn, and deterministic memo identity;
- `095-097`: canonical result and feature activation/non-activation;
- `100`: the one-page miniature solver through the production path.

The implementation follows `rodin/docs/10-identity.md` through
`50-conflict-learning.md`. In particular:

- `Version` is a records-at-offsets Vix value; field reads never reparse display
  bytes;
- `VersionSet` is a normalized union of intervals plus Cargo prerelease
  admission, not a release-only approximation;
- typed `Guard`, `Consequent`, `Gate`, `Clause`, `Domain`, `Region`, and
  `NoGood` values are used directly;
- no string tags, integer kind columns, parallel-column object model, private
  interner, maintained read-set, or private solver cache is admitted;
- branches are persistent values; no mutable trail is part of solver semantics;
- region learning preserves point -> widen -> install and containment-based
  subsumption. Missing read-set exposure falls back to declared-structure
  widening; it never guesses.

### R3 — Cargo oracle and production-shaped kernel

Before implementing the full native kernel:

1. Correct the oracle contract in rungs 098 and 100 under explicit design
   authority. `expected_selection()` recorded from the deleted resolver cannot
   certify Cargo behavior.
2. Reuse `vix/tests/rodin_fixtures.rs` to materialize real offline Cargo
   workspaces.
3. Compare selected `(source, name, version)` identities against
   `cargo generate-lockfile --offline`.
4. Compare the target-projected graph and enabled edges against
   `cargo tree -e normal,build --target ... --offline`.
5. Minimize every discrepancy into a fixture before changing solver rules.

Then implement a new native kernel from `rodin/docs/content/spec.md`.

The first kernel accepts fixture-built typed `PackageUniverse` values. This is
not the raw crates.io sparse index and not the old parallel-column `Index`.
Rows contain typed package identity, candidate version, dependency clauses,
features, cfg/target gates, source coordinates, yanked state, and policy data.

Kernel completion evidence:

- direct Cargo comparison on every accepted fixture;
- deterministic results across repeated, chaos, interpreter, and JIT runs;
- no host call for pure solver work;
- typed conflicts with source/provenance data;
- no text serialization in the kernel API;
- no dependency on `rodin/rodin.vix` or `vix/corpus-next/rodin.vix`;
- structural inspection proves the typed domain model is the live execution
  path, not unused declarations beside flattened tables.

The historical 95.6% Cargo agreement is context and a baseline, not an oracle.
The deleted Rust implementation is neither restored nor consulted.

### R4 — laziness, incrementality, and scale

After the first Cargo-matching kernel:

- `078-082`: receipts, cross-run reuse, early cutoff, projection reuse, and
  nondeterminism detection;
- `094`: unvisited package rows are not read;
- `099`: changed roots reuse untouched package work;
- `101-105`: code-edit early cutoff and lookup-not-recompute discipline;
- `124-125`: fanout and chaos differentials;
- `137-140`: trust boundary, Map scale, deep identity, and memo scale.

Measure rather than infer:

- requested/decoded package rows;
- propagated clauses and learned regions;
- candidate branches and repeated dead regions;
- store interns and memo entries;
- memo/projection hits and verification failures;
- scheduler contacts and pure host calls;
- interpreter/JIT parity;
- wall time and peak RSS on asymptotic gates.

Use traces and mechanically readable artifacts for diagnosis. Profile measured
hot paths before adding a host primitive. A host primitive is allowed only for a
measured substrate operation with a typed verified contract, never because the
deleted Rust solver had one.

### R5 — integration adapters

Only after R3 is green:

- typed Cargo.toml/workspace decoding;
- crates.io sparse rows and archived crate metadata;
- git, path, registry, replacement, and patch sources;
- target/cfg fact acquisition;
- modules (`106-110`) before the production kernel is split across files;
- human diagnostics and explicit proof values;
- structured API/rendering adapters;
- replacement of the old runnable and corpus-only Rodin files.

The sparse-index adapter produces `PackageUniverse` rows. It does not dictate
the kernel representation. Newline-delimited strings and `Doc` linked lists are
not kernel interfaces.

### R6 — full ladder completion

After native Rodin is real, return to the unselected rungs and restore the
largest consecutive prefix through all 146 rungs. The priority track changes
implementation order, not the language's final completeness standard.

## Deferred from the first kernel

These are intentionally outside the pure-kernel checkpoint unless a selected
rung proves they are genuine dependencies:

- paths and path rejection (`046-047`);
- JSON/TOML decode and external failure forms (`062-066`);
- exec, trees, fetch, and archives (`067-077`), except where the host Cargo
  oracle harness needs process execution outside Vix;
- module and diagnostic bands (`106-122`);
- effect parallelism and progressive trees (`126-130`);
- unrelated arithmetic/ordering edge semantics (`131-136`).

Deferral is not permission to emulate them locally. If the kernel truly needs a
deferred capability, promote its canonical rung into the priority track.

## Anti-workaround stop conditions

Stop, preserve a checkpoint, and report the missing capability if an
implementation would require any of the following:

- a one-entry Map or other operation used only to force/freeze a value;
- parallel maps standing in for typed rows or enums;
- string tags or numeric discriminants standing in for typed variants;
- parsing a value's display bytes on field access;
- copying a persistent collection per update;
- interning every accumulator step;
- a private solver memo, interner, read-set, scheduler, or warm-fact cache;
- a pure host call for Map, Set, Version, VersionSet, comparison, propagation,
  or search;
- an interpreter-only path or a raw unverified Weavy entry point;
- recorded expected solver output when Cargo can be invoked;
- a text bridge inside the solver kernel;
- changing a rung merely to accommodate the current implementation.

A negative checkpoint is evidence about the missing substrate, not evidence
that the researched algorithm is wrong. Commit it before changing direction.

## Checkpoint and fold discipline

- Commit early enough that no nontrivial work can be lost.
- Preserve deliberately red checkpoints when they identify the next exact seam.
- Never reset, revert, or discard another agent's work.
- Before reimplementing a capability, inventory preserved worktrees and commits;
  audit and fold sound work forward.
- Keep one authoritative integration branch for the active goal.
- Do not push unless Amos requests it.
- List an exact focused Nextest selection before running it.
- Every public Weavy/Vix change receives workspace consumer checks and strict
  Clippy without warning suppression.
- A milestone is complete only when its exact-tip reruns are captured and the
  worktree is clean.

## Milestone gates

Every milestone requires, in proportion to changed surfaces:

1. focused release tests for the selected rungs and adversarial contracts;
2. plain/chaos identity and result agreement;
3. interpreter/JIT parity, plus `WEAVY_JIT=0` fallback where execution changes;
4. `cargo check --workspace --all-targets`;
5. no-default checks for Weavy and Vix when public cfg/API surfaces change;
6. strict workspace Clippy with all relevant targets/features;
7. `cargo fmt --all -- --check` and `git diff --check`;
8. focused Dodeca coverage for requirements directly established by the slice;
9. clean Git status and committed evidence.

Global unrelated Dodeca debt is reported separately. Requirement references are
never invented to make coverage green.

## Definition of readiness for native Rodin

Native Rodin implementation begins when all of these are true:

- R0 and R1 are green;
- the miniature solver value layer in R2 is green through rung 100 on the
  production verified runtime;
- Map accumulation has an end-to-end non-quadratic proof;
- Version and VersionSet match Cargo fixtures, including prereleases;
- the Cargo oracle harness can supply expected lockfile and target-tree results;
- rungs 098/100 no longer depend on the deleted resolver's recorded selection;
- no known missing capability would force one of the anti-workaround shapes.

## Definition of the first native Rodin checkpoint

The first native kernel checkpoint is complete when:

- its public input and output are typed values, not text;
- it implements propagation, deterministic search, chronological backtracking,
  and region learning from `rodin/docs/`;
- it passes the existing Cargo fixture corpus on supported domain cases;
- unsupported Cargo-domain inputs fail explicitly rather than silently choosing
  a different solution;
- pure solving uses no host calls and no legacy evaluator;
- plain, chaos, interpreter, and JIT answers agree;
- its code contains none of the anti-workaround forms;
- sparse ingestion and rendering remain outside the kernel.

## Goal objective

The following block is intended to be copied verbatim into an agent goal:

> Advance Vix along the Rodin critical path and deliver the first native,
> Cargo-oracle-validated Rodin kernel. Preserve the canonical ratchet numbering
> and consecutive-prefix score, while adding a separately reported Rodin
> readiness track over the original rung files. Begin by auditing and folding
> the preserved rung-027 checkpoint, then complete the verified persistent
> Map/Set execution path and the selected collection, demand, failure, scale,
> Version, VersionSet, and miniature-solver rungs described in
> `rodin/EXECUTION-PLAN.md`. Correct rungs 098 and 100 under explicit design
> authority so Cargo—not a recorded result from the deleted Rust resolver—is
> the oracle. Once the readiness gates are green, implement Rodin from the
> representation-neutral contracts in `rodin/docs/`, accepting fixture-built
> typed `PackageUniverse` values and returning typed `SolveResult` or conflict
> values; do not port either existing Vix Rodin, and do not add sparse-index
> ingestion or text bridges before the kernel passes Cargo fixtures. Preserve
> and commit every meaningful checkpoint, including exact red boundaries. Stop
> and report rather than introducing forcing maps, flattened parallel columns,
> string/numeric tags, per-access reparsing, quadratic collection updates,
> per-step interning, private caches, pure host calls, raw execution paths, or
> recorded expected answers. Use the production VerifiedProgram/Executable
> path, plain/chaos and interpreter/JIT differentials, Cargo fixtures, scale
> counters, strict workspace checks, Clippy, formatting, and focused requirement
> coverage as completion evidence. Keep the integration worktree clean and do
> not push unless explicitly asked.
