# crate.vix port gaps

Source: `playgrounds/snark/src/bundled/vix/samples/crate.vix`
Port: `vix/corpus-next/crate.vix`

## Counts

- Old: 2060 lines, 194 functions, 22 exported functions.
- New: 1624 lines, 135 functions, 22 exported functions.
- Delta: -436 lines, -59 functions; exported entrypoints preserved.

## Wins

- `vix/corpus-next/crate.vix:146` introduces a shared typed `RustUnit` builder. The old no-deps/one-dep/two-deps/dynamic-deps matrix mostly dissolves into `externs`, `deps_tree`, `BuildMode`, and `CargoUnitKind`; it does not just move into renamed helper functions.
- `vix/corpus-next/crate.vix:196` and `vix/corpus-next/crate.vix:231` replace `Doc` walking for manifests and locks with typed TOML decode records. The old `doc_string`, `doc_as_string`, `doc_as_bool`, and `doc_as_strings` helpers are gone.
- `vix/corpus-next/crate.vix:40` types Cargo profile scalar fields as `Option<Int>` and `Option<Bool>` where the book gives primitive value types; stringly local int/bool conversion helpers are gone.
- `vix/corpus-next/crate.vix:855`, `vix/corpus-next/crate.vix:995`, `vix/corpus-next/crate.vix:1009`, and `vix/corpus-next/crate.vix:1049` replace `pop()` recursion with ratified `fold`, `values().any`, `filter_map`, and array spread.
- `vix/corpus-next/crate.vix:712`, `vix/corpus-next/crate.vix:725`, `vix/corpus-next/crate.vix:807`, `vix/corpus-next/crate.vix:815`, `vix/corpus-next/crate.vix:1607`, and nearby path sites use `p""` segments plus `/` joins instead of embedded slash strings.

## Gaps and awkwardness

- `vix/corpus-next/crate.vix:20`: typed TOML string-or-table enum decoding is used for `edition`, but the ratified surface does not spell out variant naming, field annotations, or how `"2021"` maps to `CargoEdition::Literal`. PROPOSAL: ratify decode attributes for string-or-table enums, including the string payload variant and table-field mapping.
- `vix/corpus-next/crate.vix:45` and `vix/corpus-next/crate.vix:58`: `panic`, `lto`, and `strip` remain strings because the book gives primitive value types but not Cargo profile domain enums. PROPOSAL: add Cargo profile enums such as `PanicStrategy`, `LtoMode`, and `StripMode`, with render-to-rustc behavior.
- `vix/corpus-next/crate.vix:220` and `vix/corpus-next/crate.vix:1327`: impossible/malformed cases use empty-map lookup as a panic device, because the designed surface has no honest typed error construction. PROPOSAL: ratify a typed `panic(message) -> Never` or `err(message) -> Result<Never>` expression, with receipts preserving the message.
- `vix/corpus-next/crate.vix:570`: `-L dependency={tree}` is awkward as `Arg::Str("dependency=")` plus an empty-subpath interpolation. PROPOSAL: ratify argv fragments that can represent a flag prefix plus a typed tree/path payload.
- `vix/corpus-next/crate.vix:596`: `compile_rust_unit` still has four `rustc!` arms for metadata/link emit combinations. The duplication moved here because path-valued emit fragments are not a banked `Arg` value. PROPOSAL: add a typed `RustcEmit`/`ArgPath` argv fragment, or make `rustc!` accept typed argv records.
- `vix/corpus-next/crate.vix:867` and `vix/corpus-next/crate.vix:1067`: `[Tree].collect()` is kept from the old corpus shape to build dependency trees, but `SURFACE.md`/collections do not ratify `Tree` collection. PROPOSAL: ratify `Tree::collect([Tree])` or a named tree union function with dependency semantics.
- `vix/corpus-next/crate.vix:1082` and `vix/corpus-next/crate.vix:1092`: `filter_map(...).sorted()` gives deterministic arrays after losing positions. That is probably fine for `--extern` and build-script cfg flags, but Cargo sometimes preserves input order in diagnostics. PROPOSAL: add an explicit `enumerate().values().filter_map(...).sorted()` pattern to examples when original order matters.
- `vix/corpus-next/rodin.vix:133`, `vix/corpus-next/index.vix:28`, and `vix/corpus-next/cargo_manifest.vix:130`: the same clause/guard/consequent/gate table shape is declared three times across corpus files because there is no module/import story for a shared corpus type. PROPOSAL: ratify cross-file modules/imports so this table can be named once and extended by sparse/workspace-specific state.
- `vix/corpus-next/crate.vix:1178`: build-script environment construction still hard-codes `CARGO_PKG_VERSION_MAJOR/MINOR/PATCH/PRE` as in the source. PROPOSAL: expose `Version` component accessors and parse package versions in the typed manifest model.
- `vix/corpus-next/crate.vix:1211` and `vix/corpus-next/crate.vix:1304`: build-script execution keeps current `build_script!` shapes. The exec-observer design wants stdout observers for readiness and directive parsing, but the observer surface is not ratified. PROPOSAL: capability-level observer defaults for build-script and rustc JSON streams, with per-call override once surface lands.
- `vix/corpus-next/crate.vix:1405`: stdout line parsing keeps the old recursive string walk because no `String::lines()`/split iterator is banked. PROPOSAL: ratify `String::lines() -> [String]` or a pure split combinator.
- `vix/corpus-next/crate.vix:1557`: proc-macro dynamic-library naming still uses local target OS matching. PROPOSAL: add a Cargo/rustc helper for proc-macro dylib filenames per host target.

## Duplication verdict

The build lane's duplication mostly dissolves. The original combinatorial helper family is replaced by one `RustUnit` builder plus small adapters from the lock, resolved-graph, solution-graph, build-script, and proc-macro domains. The remaining duplication is concentrated in two places: the four emit arms in `compile_rust_unit` (`vix/corpus-next/crate.vix:596`) and Cargo-domain boilerplate the book does not yet model (`vix/corpus-next/crate.vix:1178`, `vix/corpus-next/crate.vix:1557`). Those are real language/library gaps, not just local refactor misses.
