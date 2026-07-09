# GAPS: cargo_manifest.vix

Source: `playgrounds/snark/src/bundled/vix/samples/cargo_manifest.vix`
Port: `vix/corpus-next/cargo_manifest.vix`

## Counts

- Old corpus: 2542 lines.
- New port: 2041 lines.
- Net: 501 fewer lines, about 19.7% smaller.
- Old public Rust-demand probes: 56 `pub fn`s. New public probe surface: 0 `pub fn`s; old probe meanings are represented by 13 `#[test] fn ... -> Stream<Check>` tests at `vix/corpus-next/cargo_manifest.vix:1918`.
- Old walker helpers: 52 `*_tuple` functions. New walker helpers: 0.
- Old array removal sites: 54 `.pop()` calls and 23 `.push()` calls. New port: 0 of either spelling.
- Typed manifest/sparse decode replaces the old `Doc`-walking clusters at source lines 103-190, 1079-1091, 1638-1685, 2119-2306, and 2398-2464.

## Wins

- `vix/corpus-next/cargo_manifest.vix:48`: package inheritance is now a decoded enum instead of `Doc.is_map()` probing. PROPOSAL: keep the enum shape, but make literal `workspace = true` validation part of decode.
- `vix/corpus-next/cargo_manifest.vix:78`: Cargo's string-or-table dependency syntax is now `CargoDependencySpec`, and all detailed dependency projections share one typed path. PROPOSAL: make this the ratchet exemplar for scalar-or-table TOML decode.
- `vix/corpus-next/cargo_manifest.vix:300`: workspace member expansion is one `fold` plus array spread, replacing the old recursive entry/glob walker pair. PROPOSAL: add this exact glob-member example to the collections chapter.
- `vix/corpus-next/cargo_manifest.vix:641`: old tail-search walkers became array-order `fold(None, ...)`, preserving last-match behavior without `_tuple` helpers. PROPOSAL: document "last matching element" as a standard fold idiom or add `find_last`.
- `vix/corpus-next/cargo_manifest.vix:1014`: direct dependency clause registration is expressed as folds over members and dependency names, instead of nested manual recursion. PROPOSAL: add a solver-adapter example showing state threading through folds.
- `vix/corpus-next/cargo_manifest.vix:1918`: Rust-only demand adapters moved into native generator tests. PROPOSAL: let corpus ports keep private helpers freely callable by tests in the same module.

## Awkwardness And Ambiguity

- `vix/corpus-next/cargo_manifest.vix:57`: decode rename attributes are necessary for hyphenated TOML keys, but the book does not pin an attribute spelling. PROPOSAL: ratify `#[decode(rename = "...")]` or replace it with a single canonical field-name transform.
- `vix/corpus-next/cargo_manifest.vix:48`: `PackageScalar::Inherited { workspace: Bool }` carries a value that should be exactly `true`, but the type cannot state that. PROPOSAL: allow decode variants with required literal fields, e.g. `Inherited { workspace: true }`.
- `vix/corpus-next/cargo_manifest.vix:14`: `cfg` remains `Doc` because the book does not provide a Cargo `cfg(...)` expression type. PROPOSAL: add a typed `CfgExpr` enum and make `cfg(text)` return that, not `Doc`.
- `vix/corpus-next/cargo_manifest.vix:32`: `CargoUnitProfile` still stores booleans and null-like settings as strings to preserve the old TSV row meaning. PROPOSAL: add profile-domain enums/newtypes once the book has a newtype story.
- `vix/corpus-next/cargo_manifest.vix:689`: dependency kind dispatch is still stringly (`"normal"`, `"build"`, `"dev"`) because no `DependencyKind` type is banked. PROPOSAL: introduce a Cargo-domain enum in the corpus after the book banks domain newtypes/enums.
- `vix/corpus-next/cargo_manifest.vix:718`: target dependency lookup still indexes target tables by raw `String`; the TOML target-key grammar is not typed. PROPOSAL: add a `TargetCfg`/`TargetTableName` type and decode target tables through it.
- `vix/corpus-next/cargo_manifest.vix:810`: unique append still uses linear `Array.contains` because `Set<T>` is not in the ratified surface. PROPOSAL: add first-class `Set<T>` and replace feature-name de-dup arrays.
- `vix/corpus-next/cargo_manifest.vix:1643`: record defaults would remove nearly all repeated profile fields. PROPOSAL: add per-field defaults plus a derived/default literal, then make `registry_profile` a one-field spread.
- `vix/corpus-next/cargo_manifest.vix:1878`: JSONL parsing still uses recursive string splitting because line iterators are not banked. PROPOSAL: add `String.lines() -> [String]` or `Stream<String>` plus `filter_map(json_decode)`.
- `vix/corpus-next/cargo_manifest.vix:1927`: native tests need named fixture trees for cases that Rust currently constructs inline. PROPOSAL: document a fixture catalog mechanism, or bank a tree literal/test-fixture builder.
- `vix/corpus-next/cargo_manifest.vix:2011`: decode-failure assertions are hand-written `match` expressions because the ratified generator test surface has no `expect_error_contains` helper. PROPOSAL: add an `expect_err_contains(result, text)` helper to `/vix/testing`.
- `vix/corpus-next/cargo_manifest.vix:1911`: full Cargo metadata and Cargo.lock differential oracles from `vix/tests/cargo_manifest.rs` are not expressible in-language. PROPOSAL: add oracle fixtures as demandable values rather than host-side Rust loops.

## Missing Or Deferred Meaning

- Real-workspace tier-A timing/artifact probes from the Rust test file are intentionally not reproduced as in-language tests. PROPOSAL: specify measurement/artifact checks separately from `Check` streams so ports do not invent host effects.
- `target_shapes_array_gap` remains at `vix/corpus-next/cargo_manifest.vix:1911` to preserve the old pinned gap message. PROPOSAL: once array rendering of records is specified, replace this gap pin with a real `[CargoTargetShape]` check.
- `resolved_unit_adaptation_gap` remains at `vix/corpus-next/cargo_manifest.vix:1915` to preserve the old resolved-unit adapter limitation. PROPOSAL: after `crate.vix` is ported, introduce a shared typed `ResolvedUnit` adapter and delete the text gap.
