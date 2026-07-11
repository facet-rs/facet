# The ratchet

One hundred and forty-six rungs. Each is a Vix test file that begins red and becomes
permanently green only through the production compiler/runtime path.
When `vx test --ratchet` reports rung 100 green, the language described in
the vix book (`/vix`) exists. That is the definition of done.

## Rules of the ladder

1. **Rungs are the spec.** An implementing agent NEVER edits a rung to
   make it pass. If a rung looks wrong — contradicts the book, uses
   surface the book doesn't have — stop and report; changing a rung is a
   design decision, not a fix.
2. **The ratchet only counts consecutive green.** The score is the
   highest N such that every rung ≤ N passes. A green rung above a red
   one is progress but not score.
3. **A rung uses only surface introduced at or below it.** That's what
   makes rung N a precise target: everything it needs beyond rung N−1 is
   its own subject.
4. **`.reject.vix` rungs must fail to compile** with the diagnostic
   declared in their header. A reject file that compiles is a red rung.
5. **`.warn.vix` rungs must compile and emit** the warning and source line
   declared in their header. The warning remains a warning in the language;
   the ratchet fails because an expected diagnostic was absent, not because it
   globally promotes warnings to errors.
6. **The foundation contract binds** (FOUNDATION.md, same directory):
   traces + counters + receipts from rung 001, chaos-run agreement, and
   spec-coverage gates at band boundaries. Behavior alone does not
   advance the ratchet.
7. Test-system semantics (`#[test] fn name() -> Stream<Check>`, yielded
   `expect_*` checks, yielded trace-check calls, rerun attributes/headers,
   snapshots) are specified in the book's [Testing](/vix/testing)
   chapter. The harness is itself part of what the ladder demands into
   existence.

## Priority tracks do not reorder the ladder

A named implementation-priority track may select existing rung files above the
current consecutive red boundary. This makes an end-to-end product dependency
cone visible without renumbering the pedagogical ladder.

- A track references the original rung source; it does not copy or weaken it.
- Green track entries above a red predecessor are progress, not canonical score.
- Original surface-introduction order and the foundation contract still bind.
- A missing prerequisite is added to the track or implemented canonically; it
  is never replaced by a track-local workaround.
- Rung corrections remain explicit design decisions.

The active Rodin track, gates, and stop conditions are specified in
[`rodin/EXECUTION-PLAN.md`](../../../rodin/EXECUTION-PLAN.md).

## Fixtures the suite ships

- `fixture_tree(name)` — small file trees (`small-crate`, `touched-fixture`,
  `readme-changed`).
- `fixture_registry()` / `fixture_index()` — a tiny offline package index:
  `liba` (1.2.0, 1.3.0 — 1.3.0 depends on `libc ^2.0`), `libb` (1.9.0,
  2.0.0, 2.1.0), `libc` (1.0.0, 2.0.0), `libd` (3.x with conflicts to
  learn from), `libe` (1.0.0, optional dep `libnet` behind feature `net`),
  `libz` (never visited — asserting laziness), plus archived `.crate`
  files for fetch/extract rungs.
- `fixture_workspace("kitchen-sink")` — 12 packages, diamonds, features,
  and a recorded `expected_selection()` from the reference resolver.
- Rungs marked for rerun execute twice against one store; variants
  `rerun_with: "<fixture-mutation>"` apply the named mutation between runs.

## The rungs

| # | file | certifies |
|---|---|---|
| 001 | harness | `test` declarations, `expect` |
| 002–005 | arithmetic, bindings, functions, tuples | literals, let, fn, application, `.0` |
| 006–008 | records, enums, spread | struct/enum decl+construction, match payloads, `..s` update |
| 009–012 | equality, spaceship, comparisons, total order | ambient `==`/`<=>`, derived `<`, structural+total |
| 013 | expression-statement (reject) | values go somewhere; no statements |
| 014–017 | if/else, booleans, match, guards | `if` as expression, `\|\|` `&&` `!`, exhaustive match, arm guards |
| 018 | non-exhaustive (reject) | checker exhaustiveness |
| 019–022 | destructuring | let / match / closure params / nested record patterns |
| 023–025 | option, user enums, Ordering | `Option`, generic enums, `Ordering` is ordinary |
| 026–031 | arrays | literal/index/len, field-wise map, enumerate, fold, predicates, split_last |
| 032 | pop (reject) | mutation-shaped names don't exist |
| 033–040 | streams, maps, order values | array streams, key-preserving filter, explicit value sorting, canonical fold after sort, filter_map/flat_map gaps, find/split min/max, key roundtrip, `sorted where { order }` |
| 041–044 | maps & sets | `+` one row, addressed `get`, `has`, deliberate `with`, disjoint `++`, canonical keys, `Set<T>` |
| 045–047 | strings & paths | concat/split/parse, `p""` join-only, string→path (reject) |
| 048–052 | functions | closures capture, recursion, 100k tail loop, fold at scale, higher-order |
| 053–059 | demand semantics | args-are-wires, partial dependency, deferred match, undemanded-is-free, element independence, memo within run, distinct demands |
| 060–061 | snapshots | ambient rendering, canonical-order stability |
| 062–066 | typed decode | JSON/TOML onto structs, Option fields, string-or-table enums, failure as value |
| 067–070 | exec | run+capture, arbitrary-expression failure catch, memoized, undeclared capability (reject) |
| 071–074 | trees | projection (+never_read), glob, subfile argv, declared env |
| 075–077 | fetch & archives | pinned fetch, memoized fetch, untar+project |
| 078 | receipts | reads recorded exactly |
| 079–082 | across runs | warm reuse, early cutoff, projection reuse (the two-step dance), flakiness detected |
| 083–085 | capstone: versions & index | semver parse/order, VersionSet algebra, typed index rows |
| 086–088 | capstone: state & propagation | domains, narrowing as fresh values, conflict values |
| 089–091 | capstone: search | trivial solve, backtracking-without-trail, unsat is None |
| 092–095 | capstone: learning & discipline | learned pruning, deterministic solve, lazy index, solution snapshot |
| 096–097 | capstone: features | optional deps on/off (+never_read) |
| 098 | capstone: oracle | matches the reference resolver on kitchen-sink |
| 099 | capstone: warm restart | one req bumped; untouched subtree untouched |
| 100 | **the solver** | the book's final chapter, whole, green |

## Bands 101+ — "the language is good"

Rungs 1–100 define existence; these define quality. Same rules, same
foundation contract; the score past 100 counts consecutively as before.
New harness surface introduced here: `NNN-*.v2.vix` second-phase sources
for `rerun_with: "source-v2"` (code-edit rungs), `differential: "FLAG"`
(run twice, plain vs forced mode, results must be identical),
`failed_with` / `failure_span_in` yielded checks (asserting typed demand
failures), `overlapped` / `finished_before` / `killed` yielded checks
(parallelism and kill-when-satisfied), `memo_hits_at_least` /
`demanded_times`.

| # | band | certifies |
|---|---|---|
| 101–105 | code-edit early cutoff | body edit, same value → one node recomputes (101); changed value → downstream recomputes (102); rename = accepted cold (103); wrapper refactor recovered by suffix nomination (104); reuse is lookup, not recompute-and-compare (105) |
| 106–110 | modules | imports, visibility (reject), std across boundaries, collisions (reject), memo across module boundaries |
| 111–122 | diagnostics | twelve rejects asserting message content and span: type mismatch, arity both ways, unknown/missing/duplicate field, unknown variant, payload shape, refutable let, non-Bool guard, unresolved name, duplicate binding |
| 123–125 | differential guards | force-molten-copy, force-fanout, chaos — bit-identical results under every as-if mode |
| 126–130 | parallelism observed | overlapped effects, fan-out parallelism, progressive trees (subfile consumer finishes before producer exits), spawn-and-park, kill-when-satisfied |
| 131–136 | edge semantics | unary minus; division by zero and overflow as typed failures; float TOTAL order (NaN reflexive, sorts last); string order by codepoints; unwrap-None carries a span |
| 137–140 | trust & scale | corrupted store caught by reverify; map accumulator (molten twin of 051); identity at 100k depth; memo under 100k-demand load |
| 141–146 | collection failure & diagnostics | addressed `MissingKey`; duplicate-row and overlapping-map failures; parse failure span; `must_use` warning; old `push`/`insert` spellings rejected |
