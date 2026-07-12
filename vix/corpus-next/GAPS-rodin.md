# Rodin Corpus-Next Port Gaps

These ports are design artifacts against `vix/corpus-next/SURFACE.md`; they are
not expected to parse or run today. Correctness means preserving the meaning of
`rodin/rodin.vix` and `rodin/index.vix` while exposing where v2 helps or hurts.

## Measured Line Counts

- `rodin/rodin.vix`: 1712 lines -> `vix/corpus-next/rodin.vix`: 1377 lines, -335.
- `rodin/index.vix`: 497 lines -> `vix/corpus-next/index.vix`: 437 lines, -60.
- Combined: 2209 lines -> 1814 lines, -395.
- Std support touched for the port: `vix/std/version.vix`: 104 lines.

## Explicit Bets

- `.values()` reads as punctuation in the seven remaining sites, not ceremony: all seven are at Set/Map-to-array boundaries for deterministic key-order folds, e.g. `vix/corpus-next/rodin.vix:844`, `:849`, `:861`, `:902`, `:920`, `:974`, `:1165`. PROPOSAL: keep `.values()`; the call is useful because it marks the one compaction.
- `where { ... }` helps at small arities but buries control flow at scale. Count: 316 `where` sites across the two port files, with 11 signatures still wide enough that I would rather have records. PROPOSAL: records should be the style guide for 4+ named inputs, not just legal syntax.
- At-most-one-positional is painful in branch search: `try_candidate` at `vix/corpus-next/rodin.vix:1054` and its call at `:1050` carry `state`, `target`, `pkg`, `version`, and `rest`; this reads like a continuation record. PROPOSAL: introduce `CandidateAttempt`.
- At-most-one-positional is also painful in the sparse bridge: `add_required_dep_clauses` at `vix/corpus-next/index.vix:274` still has five named fields. PROPOSAL: introduce `RequiredDepClause` beside `SelectedGuardClause`.
- The absence of `Multiset` does not hurt the Rodin meaning. `features`, `learned`, `Region.packages`, and `Region.features` were uniqueness-maintained sets; `Set` addition (`set + x`) at `vix/corpus-next/rodin.vix:369`, `:686`, `:862`, and `:871` is clearer than sorted-array round trips.
- ~~Stdout has no home~~ **RESOLVED, round 12.** `exec` returns `ExecOutcome { tree, stdout: Stream<Int,String>, stderr }` (`r[machine.primitive.exec-outcome]`); `stdout` is a codata field, not a file. The proposal to "define an exec stdout projection or a typed captured-output field" is **answered by the returned struct**, not owed. **Still owed:** `vix/corpus-next/rodin.vix:497` keeps `--stdout {p"cfg.stdout"}` and must be re-ported onto `out.stdout` — a mechanical change, blocked on nothing.

## Gaps And Awkwardness

- `vix/std/version.vix:19`: `PreIdent` structural order only works if enum variant order is SemVer precedence. PROPOSAL: add this exact declaration to the std contract and test it with SemVer examples.
- `vix/std/version.vix:24`: `PreTag` makes `Release` structurally greater than `Prerelease`, matching SemVer, but build metadata still participates in value identity. PROPOSAL: document that precedence and value equality intentionally diverge.
- `vix/std/version.vix:96`: `same_precedence` is necessary because build metadata is retained. PROPOSAL: expose it from std next to `parse_version`.
- `vix/corpus-next/rodin.vix:494`: `Rustc::acquire(target)` now takes a demand-root supplied target, but `--target {target}` assumes the capability identity is also argv-renderable. PROPOSAL: define `Target.triple` or a render projection.
- `vix/corpus-next/rodin.vix:554`: cfg `target = ...` compares a string doc field to `Target`. PROPOSAL: decide whether cfg evaluation receives `Target` or a distinct target triple string.
- `vix/corpus-next/rodin.vix:508`: `rustc_cfg(...)` is preserved as an old parser/effect shape because v2 does not specify stdout parsing. PROPOSAL: type the rustc-cfg observation path.
- `vix/corpus-next/index.vix:95`: `fetch(url: ...)` is inherited from the old corpus; `SURFACE.md` mentions fetch but not the final named-only expression spelling. PROPOSAL: ratify `fetch where { url, sha256 }`.
- `vix/corpus-next/index.vix:59`: `SelectedGuardClause` is a win: it removes the adjacent `parent_version` / `consequent_tag` / `kind` string pile from the widest clause constructor.
- `vix/corpus-next/index.vix:274`: `add_required_dep_clauses` remains too wide after the selected-guard record extraction. PROPOSAL: make `RequiredDepClause`.
- `vix/corpus-next/rodin.vix:341`: `narrow_for_clause` is mechanically legal but not more readable than the old call. PROPOSAL: record `NarrowClause`.
- `vix/corpus-next/rodin.vix:621`: `apply_clause_id` carries state, target, clause id, and changed flag; the changed accumulator obscures the subject. PROPOSAL: record `ClausePass`.
- `vix/corpus-next/rodin.vix:1047`: `try_candidates` and `:1054` `try_candidate` want a continuation/search record. PROPOSAL: record `SearchBranch`.
- `vix/corpus-next/rodin.vix:1208`, `:1231`, `:1250`: selected-name rendering functions carry packages, selected, and output accumulators. PROPOSAL: record `SelectedRender`.
- `vix/corpus-next/rodin.vix:838`: Set containment becomes `set.map(...).values().fold(...)`. It is semantically clear but longer than `fold_ascending`. PROPOSAL: add `Set::all` / `Set::any` as key-order-independent reducers.
- `vix/corpus-next/rodin.vix:894`: region unsatisfied counting uses a mapped contribution array plus sentinel folding. PROPOSAL: add `Map/Set::try_fold_values` or a conflict-aware reducer.
- `vix/corpus-next/rodin.vix:913`: first unsatisfied package uses a sentinel instead of `find_min`. PROPOSAL: add `Set::find_first` explicitly defined by structural key order.
- `vix/corpus-next/rodin.vix:972`: learned no-good propagation now spells the deterministic set order as `map(...).values().fold(...)`. PROPOSAL: if Set folding is allowed, specify it directly instead of forcing identity maps.
- `vix/corpus-next/rodin.vix:1191`: public `solve` takes `Target`, preserving the v2 host ruling. PROPOSAL: demand roots should supply `Target` in examples so corpus entry points do not invent it.
- `vix/corpus-next/index.vix:365`: sparse solve also takes `Target`; this preserves the new Rodin API but makes old string-target fixtures under-specified. PROPOSAL: add fixture-side target construction.

## Wins

- Deleted `namespace Version { fn <=> }`; `Version` now gets precedence from its std declaration.
- Replaced all Rodin `Multiset<T>` storage with `Set<T>`.
- Replaced the three sorted-array uniqueness round trips with `Set` addition (`set + x`).
- Removed `Target::host()` and threaded a supplied `Target` through the resolver entry points.
- Converted local free-function calls to the v2 subject plus `where { ... }` convention.
- Introduced `SelectedGuardClause` for the worst sparse-index wide call.
- Preserved the load-bearing comments and updated wording where the surface changed underneath them.

## Comment Adaptations

- `vix/corpus-next/rodin.vix:14`: changed the old v1 `Multiset` workaround note to a closed `Set` note.
- `vix/corpus-next/rodin.vix:1359`: changed the operator-overload smoke test to a structural-order smoke test because `<=>` is no longer overridable.
- `vix/std/version.vix:3`: changed the std comment from “a type defines `fn <=>`” to “`<=>` is structural and not overridable.”
