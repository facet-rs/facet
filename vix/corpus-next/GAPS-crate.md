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
- **RULED (round 12).** `exec` returns
  `ExecOutcome { tree: Tree, stdout: Stream<Int,String>, stderr: Stream<Int,String> }`
  (`r[machine.primitive.exec-outcome]`). `stdout`/`stderr` are codata fields — read them
  while the process runs. **There is no exit status**, and asking for one was the wrong
  request: an exit code is a naked `Int` where a typed outcome belongs. A nonzero exit is a
  `fail` (`r[machine.primitive.exit-status-is-not-a-value]`), so "bad directives" (a parse
  failure over `out.stdout`) and "bad processes" (a failed `exec`) are already distinct —
  they are different demands, and each carries its own span and demand chain.
  **STILL OPEN, and it blocks the `grep`-shaped case only**: how an *accepted* nonzero exit
  becomes a typed result. `ExecOutcome` has nowhere to put it. This file does not need it.
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
- `vix/corpus-next/crate.vix:726`, `:881`, `:972`, and `:1255`: `tree_union` keeps the old
  array-to-tree union meaning. **`Tree` is NOT `Map<Path, Blob>`**
  (`r[machine.identity.tree-model]`): it is a recursive `Map<Name, TreeEntry>` over
  `File { content: Blob, executable: Bool } | Dir(Tree) | Symlink { target }`.
  PROPOSAL (still open): ratify `Tree::union([Tree]) -> Tree`, and state its semantics for
  a **name collision across entry kinds** — file-over-dir, dir-over-symlink, and two files
  with different `executable` bits. A flat-map union has nothing to say about any of those.
- `vix/corpus-next/crate.vix:1268` and `:1289`: stream `filter_map` is used because
  this file naturally filters build-script units while mapping to artifacts.
  PROPOSAL: bank `Stream::filter_map` or document the `map Option` + `filter` +
  unwrap pattern.
- **RESOLVED (round 12), re-port owed.** `crate.vix:1442` and `:1583` still carry
  `--stdout {p"build.stdout"}` (and `rodin.vix:497` carries `--stdout {p"cfg.stdout"}`).
  stdout **has a home**: `out.stdout`, a codata field. Delete the fake output file and the
  `// GAP:` comments at `crate.vix:1441` and `:1582`.
- `vix/corpus-next/crate.vix:1397`: `build_script_env` sets `HOST=` from `target_name`.
  **HOST is not cost-plane.** Cargo's `HOST` is the *selected toolchain's execution ABI* — a
  pinned semantic property of the toolchain, which enters exec identity
  (`r[machine.primitive.exec-probed-toolchain]`) and constrains scheduler admissibility
  (`r[machine.placement.capability-requirements-are-derived]`). What is cost-plane is the
  **physical executor**, which no recipe may name. PROPOSAL: `build_script_env` takes the
  semantic `target` and the acquired toolchain, and reads `HOST` off the toolchain — never
  off the machine, and never off `target_name`.
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

## GAP (round-10 backtick sweep): `build_script` is a String, not a capability

`crate.vix:1443` and `:1573` bind `let build_script = "build-script-runner";` — a
**String** — and the old `build_script! { … }` macro shadowed it. Under the ruled
form, `exec build_script`…`` tags the command with that String.

A command's tag must be a **capability value** (an identity some machine can
materialize), not a name. So either:

1. `BuildScript::acquire(…)` exists and yields a capability, or
2. the build-script runner is not a capability at all but an ordinary executable
   produced by a previous `exec` — in which case the tag is a `Tree` subpath, and
   the surface must say a command may be tagged by an artifact as well as by a
   capability.

(2) is the interesting one: a build script *is* a binary you just compiled. It has
an identity, but nobody advertised it. **PROPOSAL: a command may be tagged by any
value with an identity that resolves to an executable — a capability (advertised) or
an artifact (produced).** That would also cover `objcopy`-on-your-own-output.


## RESOLVED (round 11): the failure surface exists

`fail <payload>` landed. Both empty-map error-raises in this file are ported:

- `crate.vix:232` -> `fail MissingWorkspaceField { field: "workspace.package.edition" }`
- `crate.vix:1603` -> `fail MalformedBuildScriptDirective { line }`

The machine attaches the subject's identity, the source span and the demand chain;
the payload is all you supply. `m.get(k).unwrap()` as an error-raise — which this
file, `cargo_manifest.vix` and `index.vix` each invented independently, in three
different disguises — is now a thing you should never write. Queue item C3 is closed.
See `/vix/errors`.


