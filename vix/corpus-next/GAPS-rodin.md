# Rodin Corpus-Next Port Gaps

These ports are design artifacts against `vix/corpus-next/SURFACE.md`; they are
not expected to parse or run today.

## Measured Line Counts

- `rodin/rodin.vix`: 1712 lines -> `vix/corpus-next/rodin.vix`: 1303 lines, -409.
- `rodin/index.vix`: 497 lines -> `vix/corpus-next/index.vix`: 413 lines, -84.
- Combined: 2209 lines -> 1716 lines, -493.

## Gaps And Awkwardness

- `vix/corpus-next/rodin.vix:107`: `State` still uses several bare `Int` ids across package, feature, guard, clause, and version namespaces. PROPOSAL: surface `PkgId`, `FeatureId`, `GuardId`, `ClauseId`, and `VersionId` newtypes before the next solver port.
- `vix/corpus-next/rodin.vix:133`: `Index` remains a parallel-column table keyed by bare ids, so type safety relies on naming discipline. PROPOSAL: add source-level newtypes and typed `Map<Id, T>` aliases for table-shaped indexes.
- `vix/corpus-next/rodin.vix:155`: `Problem.root_pkg` and `Problem.root_default_feature` are bare `Int` ids. PROPOSAL: surface newtypes for root package and feature ids instead of preserving the old integer wire shape.
- `vix/corpus-next/rodin.vix:199`: empty multisets are expressed as `[].values()`. PROPOSAL: add a direct empty multiset literal with type ascription, or document `[].values()` as the canonical empty multiset form.
- `vix/corpus-next/rodin.vix:322`: adding one feature to a multiset takes `[..state.features.sorted(), feature].values()`. PROPOSAL: add `Multiset::insert_one` or a true `Set<T>` with `.insert`.
- `vix/corpus-next/rodin.vix:438`: rustc cfg output is a dynamic `Doc` linked list, so this remains recursive instead of combinator-shaped. PROPOSAL: make `rustc_cfg` return `[String]` or expose `Doc::as_array`.
- `vix/corpus-next/rodin.vix:447`: `Rustc::acquire` plus the `rustc!` capability macro are not covered by `SURFACE.md`. PROPOSAL: add a capability/effect-expression chapter or keep this exact old shape as the blessed escape hatch for corpus ports.
- `vix/corpus-next/rodin.vix:490`: cfg expressions are decoded by string tags in `Doc`. PROPOSAL: expose a typed `CfgExpr` enum parser so the target matcher can use normal enum matches.
- `vix/corpus-next/rodin.vix:635`: region package insertion uses sorted multiset round-tripping to preserve uniqueness. PROPOSAL: add `Set<T>` for genuinely unique unordered collections.
- `vix/corpus-next/rodin.vix:648`: `exact_version_set` has to simulate `find_map` with an `Option` accumulator. PROPOSAL: add `Array::find_map` or a clearly named `find_last_map` for order-sensitive arrays.
- `vix/corpus-next/rodin.vix:750`: `gate_target_same` preserves the old "only both absent are equal" behavior because target expression equality semantics were not designed here. PROPOSAL: decide whether `Option<String>` equality should be used for gate effects.
- `vix/corpus-next/rodin.vix:814`: installing a learned fact converts a multiset to sorted array and back just to append one fact. PROPOSAL: add `Multiset::union_one` or use `Set<LearnedNoGood>`.
- `vix/corpus-next/rodin.vix:915`: learned no-good propagation uses `fold_ascending` with a `Step` accumulator to carry conflict short-circuit state. PROPOSAL: add `try_fold_ascending` for deterministic stop-on-conflict constraint passes.
- `vix/corpus-next/rodin.vix:988`: candidate search remains recursive with `split_last` because the solver must demand one branch at a time. PROPOSAL: add an order-sensitive `Array::try_find_rev`/`find_map_rev` for demand-selective search.
- `vix/corpus-next/rodin.vix:1199`: selected version rendering repeats the `Option` accumulator search pattern. PROPOSAL: add `Array::find_map` and document whether it is field-order or reverse-order.
- `vix/corpus-next/index.vix:81`: `fetch(url: ...)` is inherited from the old corpus but not specified by the ratified surface. PROPOSAL: document fetch as a capability-returning expression, or require typed std wrappers for index snapshots.
- `vix/corpus-next/index.vix:90`: JSONL parsing stays recursive over `String.before/after` because no line-splitting collection API is banked. PROPOSAL: add `String::lines() -> [String]`.
- `vix/corpus-next/index.vix:109`: sparse rows still use `json(line)` into `Doc` rather than typed `json_decode`. PROPOSAL: specify the decode form for dynamic crates.io rows or port this to `json_decode<SparseIndexRow>`.
- `vix/corpus-next/index.vix:147`: preserving old `pop` order requires `rows.fold([], |reversed, row| [row, ..reversed])` before the bridge folds. PROPOSAL: add `Array::reversed()` or `fold_descending`.
- `vix/corpus-next/index.vix:331`: root problem construction keeps bare package and feature ids. PROPOSAL: same newtype surface as the solver core.
- `vix/corpus-next/index.vix:359`: `find_sparse_row` uses an empty-row sentinel because the public helpers return a row, not `Option<SparseIndexRow>`. PROPOSAL: make lookup helpers return `Option` or add `Array::find_rev`.
- `vix/corpus-next/index.vix:378`: empty `Doc` values are still made with stringly `json("{}")` / `json("null")`. PROPOSAL: add `Doc::object_empty()` and `Doc::null`, or require typed decode defaults.

## Wins

- Removed the `stored_*` one-entry-map laundering entirely; the port uses direct values at domain, state, gate, clause, hypothesis, and learned-fact boundaries.
- Replaced boolean-match pyramids with `if`/`else`, `&&`, `||`, and `!`.
- Replaced tuple trampoline helpers with tuple destructuring in `split_last` matches and closure parameters.
- Replaced recursive array walkers with `map`, `fold`, `any`, `all`, `contains`, `split_last`, `sorted`, and `fold_ascending` where order semantics allowed it.
- Replaced old typed empty maps with `%{}`.
- Added `namespace Version { fn <=>(self, other) }` and changed version comparison call sites to operators.
- Used `Multiset` for unordered learned facts, region packages, enabled features, and feature polarity sets; used `fold_ascending` for multiset folds.
- Used backtick interpolation for constructed req strings, cfg fact keys, fetched sparse paths, and selected-version output lines.
