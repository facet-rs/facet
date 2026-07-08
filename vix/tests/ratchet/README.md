# The ratchet

One hundred rungs. Each is a vix test file that does not compile today.
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
5. Test-system semantics (declarations, `expect_*`, the `expecting`
   trace clauses, `//! rerun` two-phase runs, snapshots) are specified in
   the book's [Testing](/vix/testing) chapter. The harness is itself part
   of what the ladder demands into existence.

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
- Rungs marked `//! rerun` execute twice against one store; variants
  `rerun with: <fixture-mutation>` apply the named mutation between runs.

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
| 033–040 | multiset | values(), filter, canonical order, canonical fold, filter_map/flat_map, find/take min/max, Indexed roundtrip, sorted_by |
| 041–044 | maps & sets | by-value insert/get, overwrite, canonical keys, `Set<T>` |
| 045–047 | strings & paths | concat/split/parse, `p""` join-only, string→path (reject) |
| 048–052 | functions | closures capture, recursion, 100k tail loop, fold at scale, higher-order |
| 053–059 | demand semantics | args-are-wires, partial dependency, deferred match, undemanded-is-free, element independence, memo within run, distinct demands |
| 060–061 | snapshots | ambient rendering, canonical-order stability |
| 062–066 | typed decode | JSON/TOML onto structs, Option fields, string-or-table enums, failure as value |
| 067–070 | exec | run+capture, failure-as-result, memoized, undeclared capability (reject) |
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
