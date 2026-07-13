# GAPS: cargo_manifest.vix

Source: `playgrounds/snark/src/bundled/vix/samples/cargo_manifest.vix`
Port: `vix/corpus-next/cargo_manifest.vix`
Surface: v2, from `vix/corpus-next/SURFACE.md`, `vix/docs/content-design/maps-all-the-way-down.md`, and `vix/docs/content-design/spec-changelog.md` rounds 7-9 plus addenda.

## Counts

- Original v0/vRust-shaped corpus source: 2542 lines.
- Previous v1 corpus port before this re-port: 2041 lines.
- Current v2 port: 2238 lines.
- Net vs original source: 304 fewer lines, about 12.0% smaller.
- Net vs v1 port: 197 more lines, about 9.7% larger.
- Old public Rust-demand probes: 56 `pub fn`s. Current public probe surface: 0 `pub fn`s; old probe meanings are represented by 13 `#[test] fn ... -> Stream<Check>` tests at `vix/corpus-next/cargo_manifest.vix:2116`.
- Old walker helpers: 52 `*_tuple` functions. Current walker helpers: 0.
- Old array removal sites: 54 `.pop()` calls and 23 `.push()` calls. Current port: 0 of either spelling.
- Current `where { ... }` sites: 13. I would rather have a domain record at 31 remaining 5+-parameter function declarations; the worst remaining one is `vix/corpus-next/cargo_manifest.vix:1876`.

## Bets

- `.values()` reads as punctuation in `vix/corpus-next/cargo_manifest.vix:373`: `glob -> map -> collect -> values` is the exact map-model sentence. It reads as ceremony in `vix/corpus-next/cargo_manifest.vix:911` and `vix/corpus-next/cargo_manifest.vix:915`, where a `Set<String>` is immediately compacted back to `[String]` only because later text renderers still want arrays. PROPOSAL: add a first-class `Set<T>.join(sep)` or a Cargo feature-set rendering example so dedup does not have to bounce through an array.
- `where { ... }` helps for defaulted target gates at `vix/corpus-next/cargo_manifest.vix:534`, `vix/corpus-next/cargo_manifest.vix:625`, `vix/corpus-next/cargo_manifest.vix:650`, `vix/corpus-next/cargo_manifest.vix:675`, and `vix/corpus-next/cargo_manifest.vix:701`. It would bury meaning for wide builders, so those became records at `vix/corpus-next/cargo_manifest.vix:196`, `vix/corpus-next/cargo_manifest.vix:216`, and `vix/corpus-next/cargo_manifest.vix:255`. PROPOSAL: document "one subject record plus a small defaulted `where` tail" as the idiom for threaded builder state.
- At-most-one-positional is painful at `vix/corpus-next/cargo_manifest.vix:1876`: `solution_units_text_for_selected_package(...)` still wants workspace, sparse rows, solver state, result, target, package, version, and output accumulator. PROPOSAL: introduce `SolutionSelectedPackageText` and pass one record once the adjacent solution helpers are ported too.
- The absence of `Multiset` does not hurt this file as a bag; there were no true multiplicities. It does hurt feature closure ergonomics at `vix/corpus-next/cargo_manifest.vix:911` because ordered text output still wants a compact array after uniqueness is enforced. PROPOSAL: keep using `Set<String>` for uniqueness, but specify feature-list rendering order explicitly.
- `glob`-as-a-stream costs three visible calls at `vix/corpus-next/cargo_manifest.vix:375`: `.map`, `.collect()`, `.values()`. It buys a visible correctness property: workspace member arrays are now path-key ordered instead of `readdir` ordered. PROPOSAL: add this exact Cargo workspace member expansion to the collections chapter as the reproducibility exemplar.

## Wins

- `vix/corpus-next/cargo_manifest.vix:78`: Cargo's string-or-table dependency syntax remains `CargoDependencySpec`, and all detailed dependency projections share one typed path. PROPOSAL: make this the book's scalar-or-table decode chapter example.
- `vix/corpus-next/cargo_manifest.vix:373`: workspace glob expansion now treats directory listing as `Stream<Path, Path>` and only creates `[String]` after `collect().values()`. PROPOSAL: ratify `glob -> map -> collect -> values` as the common path for reproducible filesystem fan-out.
- `vix/corpus-next/cargo_manifest.vix:534`: `register_feature` now carries `target: Option<String> = None`; the old `register_feature_target` twin is gone. PROPOSAL: keep defaults for target gates and reject twin helper pairs in future ports.
- `vix/corpus-next/cargo_manifest.vix:625`, `vix/corpus-next/cargo_manifest.vix:650`, `vix/corpus-next/cargo_manifest.vix:675`, and `vix/corpus-next/cargo_manifest.vix:701`: the old selected-guard, selected-feature, feature-expansion, and required-dependency `_target` pairs are one defaulted function each. PROPOSAL: add a corpus lint for sibling functions differing only by an optional target argument.
- `vix/corpus-next/cargo_manifest.vix:1171` and `vix/corpus-next/cargo_manifest.vix:1350`: deleted `enumerate`/`Indexed`/`sorted` take helpers became explicit fold state. PROPOSAL: bank `Array.take(n)` for authored or already-sorted arrays; this is not a stream operation.
- `vix/corpus-next/cargo_manifest.vix:1853`: `solution_member_units_text` is now a single record argument, not nine positional arguments. PROPOSAL: use this as the positive example for recordifying wide call surfaces.

## Awkwardness And Ambiguity

- `vix/corpus-next/cargo_manifest.vix:57`: decode rename attributes are necessary for hyphenated TOML keys, but the book does not pin an attribute spelling. PROPOSAL: ratify `#[decode(rename: "...")]` or replace it with one canonical field-name transform.
- `vix/corpus-next/cargo_manifest.vix:48`: `PackageScalar::Inherited { workspace: Bool }` carries a value that should be exactly `true`, but the type cannot state that. PROPOSAL: allow decode variants with required literal fields, e.g. `Inherited { workspace: true }`.
- `vix/corpus-next/cargo_manifest.vix:14`: `cfg` remains `Doc` because the book does not provide a Cargo `cfg(...)` expression type. PROPOSAL: add a typed `CfgExpr` enum and make `cfg(text)` return that, not `Doc`.
- `vix/corpus-next/cargo_manifest.vix:32`: `CargoUnitProfile` still stores booleans and null-like settings as strings to preserve the old TSV row meaning. PROPOSAL: add profile-domain enums/newtypes once the book has a newtype story.
- `vix/corpus-next/cargo_manifest.vix:196`: `WorkspaceSelectedGuardClause` is longer than the old call, but clearer than a nine-field `where` block. PROPOSAL: allow record type aliases generated from function named-argument records so this boilerplate does not have to be hand-written.
- `vix/corpus-next/cargo_manifest.vix:216`: `WorkspaceFeatureExpansionClause` exists only because named args must not license wide signatures. PROPOSAL: add a style rule: builder functions over 4 inputs take a nominal record.
- `vix/corpus-next/cargo_manifest.vix:255`: `SolutionMemberUnitsText` preserves the old `workspace` and `workspace_root` inputs even though the body does not use them. PROPOSAL: after the crate/unit adapter is ported, remove unused context from the record instead of perpetuating the old signature.
- `vix/corpus-next/cargo_manifest.vix:373`: method-call syntax around `workspace.glob (pattern)` remains awkward because the surface examples keep both juxtaposition and dotted method calls. PROPOSAL: specify whether methods with one argument are written `.glob pattern` or `.glob(pattern)` in the final grammar.
- `vix/corpus-next/cargo_manifest.vix:618`: `workspace_gate_targets_insert` still takes three positional arguments; it is small enough not to recordify, but the call convention is still not visually settled for helper functions. PROPOSAL: document a threshold for inline structural `where` versus nominal records.
- `vix/corpus-next/cargo_manifest.vix:789`: dependency kind dispatch is still stringly (`"normal"`, `"build"`, `"dev"`) because no `DependencyKind` type is banked. PROPOSAL: introduce a Cargo-domain enum in the corpus after the book banks domain enums.
- `vix/corpus-next/cargo_manifest.vix:818`: target dependency lookup still indexes target tables by raw `String`; the TOML target-key grammar is not typed. PROPOSAL: add a `TargetCfg`/`TargetTableName` type and decode target tables through it.
- `vix/corpus-next/cargo_manifest.vix:907`: set construction from arrays is hand-written. PROPOSAL: add `[T].to_set() -> Set<T>` or specify `array.stream().collect()` for element-keyed sets.
- `vix/corpus-next/cargo_manifest.vix:911`: feature dedup changed from linear first-seen array order to set key order before returning `.values()`. PROPOSAL: state whether Cargo feature output should be structural-order text or authored-discovery-order text; the current v2 port chooses structural-order uniqueness.
- `vix/corpus-next/cargo_manifest.vix:1171`: `take_strings` got longer after deleting `enumerate`, because the surface has no array prefix operation. PROPOSAL: add `Array.take(n)`.
- `vix/corpus-next/cargo_manifest.vix:1350`: `take_versions` duplicates the same fold-state shape as `take_strings`. PROPOSAL: make `Array.take` generic and delete both helpers.
- `vix/corpus-next/cargo_manifest.vix:1803`: record defaults would remove nearly all repeated profile fields. PROPOSAL: add per-field defaults plus a derived/default literal, then make `registry_profile` a one-field spread.
- `vix/corpus-next/cargo_manifest.vix:1876`: `solution_units_text_for_selected_package` remains a long positional adapter because recordifying every adjacent solution function would dominate this port. PROPOSAL: introduce one `SolutionTextContext` record shared by the remaining solution text functions.
- `vix/corpus-next/cargo_manifest.vix:2075`: JSONL parsing still uses recursive string splitting because line iterators are not banked. PROPOSAL: add `String.lines() -> [String]` or `Stream<Int, String>` plus `filter_map(json_decode)`.
- `vix/corpus-next/cargo_manifest.vix:2116`: native tests need named fixture trees for cases that Rust currently constructs inline. PROPOSAL: document a fixture catalog mechanism, or bank a tree literal/test-fixture builder.
- `vix/corpus-next/cargo_manifest.vix:2223` and `vix/corpus-next/cargo_manifest.vix:2227`: decode-failure assertions still use `assert_eq(false, true)` because `fail` is not banked. PROPOSAL: queue item C3 should add a failure surface or an `expect_err_contains(result, text)` check helper.
- `vix/corpus-next/cargo_manifest.vix:2184`: full Cargo metadata and Cargo.lock differential oracles from `vix/tests/cargo_manifest.rs` are not expressible in-language. PROPOSAL: add oracle fixtures as demandable values rather than host-side Rust loops.

## Missing Or Deferred Meaning

- Real-workspace tier-A timing/artifact probes from the Rust test file are intentionally not reproduced as in-language tests. PROPOSAL: specify measurement/artifact checks separately from `Check` streams so ports do not invent host effects.
- `target_shapes_array_gap` remains at `vix/corpus-next/cargo_manifest.vix:2108` to preserve the old pinned gap message. PROPOSAL: once array rendering of records is specified, replace this gap pin with a real `[CargoTargetShape]` check.
- `resolved_unit_adaptation_gap` remains at `vix/corpus-next/cargo_manifest.vix:2112` to preserve the old resolved-unit adapter limitation. PROPOSAL: after `crate.vix` is ported, introduce a shared typed `ResolvedUnit` adapter and delete the text gap.
- `target: String` parameters in solver-facing functions still mean Cargo cfg target text, not ambient host observation. PROPOSAL: reserve `Target` for demand-root compilation targets and add a separate `CfgTarget`/`CargoTargetExpr` type for Cargo conditional dependency gates.
