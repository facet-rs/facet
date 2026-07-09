# crate.vix v2 port gaps

Source: `playgrounds/snark/src/bundled/vix/samples/crate.vix`
Port: `vix/corpus-next/crate.vix`

These are design artifacts against `vix/corpus-next/SURFACE.md`; they are not
expected to parse or run today.

## Counts

- Original source: 2060 lines, 194 functions, 22 exported functions.
- v1 corpus-next port: 1624 lines, 135 functions, 22 exported functions.
- v2 port: 1953 lines, 137 functions, 22 exported functions.
- Original -> v2 delta: -107 lines, -57 functions; exported entrypoints preserved.
- v1 -> v2 delta: +329 lines, +2 functions. The increase is almost entirely
  `where { ... }` call surface, target/toolchain separation, and explicit old
  shapes for unbanked effects.

## Wins

- `vix/corpus-next/crate.vix:150` keeps `RustUnit` as the central record model for
  crate compilation. This remains the file's best "modeling with records" example:
  `externs`, `deps_tree`, `BuildMode`, and `CargoUnitKind` still replace the old
  194-function helper matrix.
- `vix/corpus-next/crate.vix:146`, `:153`, `:634`, and `:647` separate toolchain
  identity from artifact target: `Rustc::acquire(unit.toolchain)` names a
  toolchain, while `--target {unit.target}` carries semantic target intent.
- `vix/corpus-next/crate.vix:1034`, `:1057`, `:1426`, `:1470`, `:1549`, `:1889`,
  and `:1949` remove every ambient `Target::host()` read. Proc macros, build
  scripts, standalone build-script probes, and cross-target smoke tests now take
  target from the demand root.
- `vix/corpus-next/crate.vix:1268` and `:1289` preserve dependency provenance by
  streaming authored dependency arrays before `collect().values()`, instead of
  sorting artifacts by value.

## Explicit bets

- `.values()` reads as punctuation at `vix/corpus-next/crate.vix:1278` and
  `vix/corpus-next/crate.vix:1295`: after a stream pipeline, `collect().values()`
  clearly says "determinize by key, then compact." It would read as ceremony if
  it appeared on ordinary authored arrays; this file now has only those two real
  v2 compaction sites.
- `where { ... }` helps for one or two named parameters, but buries wide build
  context. This port has 87 `where` signatures; I would rather have named records
  at 24 of them, especially `manifest_rust_unit`, `lock_lib`, `lock_bin`, all
  `resolved_*` compile/artifact helpers, all `solution_*` compile/artifact helpers,
  and `build_script_env`.
- At-most-one-positional is painful at `vix/corpus-next/crate.vix:1310`:
  `solution_compile_rust_unit target where { source, index, result, targets,
  target_name, unit, mode }` wants a `SolutionBuildCtx` plus a small operation
  record. It is also painful in record-spread calls such as
  `vix/corpus-next/crate.vix:771`, where `..(manifest_rust_unit target where { ... })`
  is faithful but visually heavy.
- `exec` needs to return at least: a `Tree` of files, stdout text, stderr or
  diagnostics streams, exit status, and structured readiness/protocol events for
  build-script stdout and rustc diagnostics. This file consumes the `Tree` for
  artifacts and `out/`, consumes stdout as text for Cargo directives, and needs
  failure/status to distinguish bad directives from bad processes. PROPOSAL: keep
  returning `Tree` for now; design a process effect result that exposes stdout,
  stderr/diagnostics, status, and tree without routing stdout through a fake file.
- Removing `Target::host()` forces target through three long chains:
  `crate_solution_bin* -> solution_unit_artifact -> solution_unit_built ->
  solution_{build_script,proc_macro} -> solution_compile_rust_unit`,
  `crate_build_script_* -> build_script_compile -> compile_rust_unit`, and
  `crate_proc_macro_cross_bin -> crate_proc_macro_bin -> proc_macro_*`.
  Those chains want records: `ResolvedBuildCtx`, `SolutionBuildCtx`, and
  `BuildScriptRunCtx`.

## Gaps and awkwardness

- `vix/corpus-next/crate.vix:202`, `:277`, `:389`, `:403`, `:424`, and `:1599`:
  zero-argument helper functions and calls remain in the old shape because v2
  bans zero-arg `!` but does not specify zero-arg function invocation. PROPOSAL:
  ratify constants for pure zero-input helpers, or a unit argument spelling.
- `vix/corpus-next/crate.vix:235` and `:1616`: impossible/malformed cases still
  use empty-map `.get(...).unwrap()` to raise an error. PROPOSAL: ratify queue item
  C3 as a typed failure surface with a receipted message.
- `vix/corpus-next/crate.vix:643`: `compile_rust_unit` still has four near-identical
  `rustc!` arms because path-valued `--emit=` fragments are not values. PROPOSAL:
  add typed argv fragments for `--emit` records, or let `rustc!` accept typed
  argv records.
- `vix/corpus-next/crate.vix:647`, `:660`, `:675`, and `:688`: `--target
  {unit.target}` preserves target semantics, but the surface does not define
  `Target -> rustc target triple` rendering. PROPOSAL: add a `Target::rustc_triple`
  projection or a typed rustc target argument.
- `vix/corpus-next/crate.vix:726`, `:881`, `:972`, and `:1255`: `tree_union` keeps
  old array-to-tree union meaning because v2 only says `Tree = Map<Path, Blob>`.
  PROPOSAL: ratify `Tree::union([Tree]) -> Tree` with duplicate-path semantics.
- `vix/corpus-next/crate.vix:1268` and `:1289`: stream `filter_map` is used because
  this file naturally filters build-script units while mapping to artifacts.
  PROPOSAL: bank `Stream::filter_map` or document the `map Option` + `filter` +
  unwrap pattern.
- `vix/corpus-next/crate.vix:1450` and `:1592`: stdout still has no home; both
  build-script exec calls keep `--stdout {p"build.stdout"}` loudly. PROPOSAL:
  resolve stdout in the effect model before changing this shape.
- `vix/corpus-next/crate.vix:1397`: `build_script_env` still sets `HOST=` from
  `target_name`, preserving the old fixture meaning but not modeling host as a
  separate cost-plane input. PROPOSAL: make Cargo build-script env construction a
  typed std helper that receives explicit semantic target and host-policy inputs.
- `vix/corpus-next/crate.vix:1885`: proc-macro dynamic library naming still derives
  from the supplied target. PROPOSAL: expose Cargo/rustc proc-macro artifact naming
  instead of local OS matching.
- `vix/corpus-next/crate.vix:1823`, `:1856`, `:1865`, and `:1949`: adding
  `target` to `crate_build_script_*` and `crate_proc_macro_cross_bin` changes
  public probe signatures. PROPOSAL: demand roots should pass target explicitly;
  fixture adapters can supply host-defaulted target outside the language.

## Commentary adaptations

- Added inline `GAP` comments at `vix/corpus-next/crate.vix:233`, `:1449`, `:1591`,
  and `:1614` for the two failure sentinels and two stdout fake-file sites.
- Adapted the target comments implicitly by deleting `Target::host()` rather than
  describing a host read. The remaining `target` parameter is semantic recipe input.
