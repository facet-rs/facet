# The package manifest seam (design fiction)

`package.styx` is vix's flake.nix / package.json — except it takes the flake
architecture (literal inputs + outputs as pure functions of inputs) and puts
the data/code boundary at a *file* boundary instead of policing it inside one
file with a restricted evaluator mode. The manifest is dead data read by
`facet_styx::from_str` with zero evaluation; every computed thing lives in a
`.vix` fn the manifest merely names.

Two packages here, the two sides of one seam:

- `facet/` — a source package: `package.styx` + `facet.vix`. Declares a
  `@package` input (provider pin), two commands, an artifact, and an exposed
  program carved out of that artifact.
- `cargo/` — a tool package: pinned `@fetch` input, programs carved out of the
  fetched toolchain tree, and the commands that make `vx r cargo build` the
  no-config cargo build.

## The data model, complete

Three shapes and nothing else — no computation anywhere in the manifest:

1. **pins** — `input <name> @package{…} | @fetch{…}`. A discriminated union
   (styx variant tags, per facet-styx's `@tag{…}` decode). `@fetch` decodes
   toward `PinnedBlobRef`; `@package` pins a provider package by source-tree
   identity, which transitively covers that package's own manifest and
   therefore its pins. Deliberately no `@path` arm: local overrides are
   policy (`vx r --override-input`), not recipe.
2. **named fn refs** — `command <name> { fn <module::fn> }` and
   `artifact <name> { fn <module::fn> }`. Same shape, different verb and
   return-type discipline: artifacts must return Tree|Blob (checked at
   binding, per vixen.delivery.result); commands return whatever `vx r` can
   present. Purity means an artifact is nothing but its fn's memoized value.
3. **paths into values, plus contracts** — `program <name> { from <input> |
   from-artifact <artifact> "<path>" … }`. The on-disk spelling of a
   capability package (vixen.capability.package-is-data). `from` vs
   `from-artifact` is invisible to consumers: fetched and built-here trees
   provide capabilities identically.

Rules inherited, not invented: requirements come from fn signatures only —
inputs are provider pins, never a `[dependencies]` list
(requirements-from-use); binding failures precede all effects; no lockfile —
few inline pins here, many pins in the ecosystem's own lockfile (pins.md).

## `vx r <pkg> <command>` resolution rungs

1. cwd inside a package whose manifest declares `<pkg>` (or is it): that pin.
2. otherwise the machine manifest's default registry pin — the no-config
   path; a bare cargo workspace hits this rung.
3. `vx r cargo@1.89 build` overrides explicitly.

Once a manifest exists, rung 2 is off for that package's own fns: writing a
manifest means stating your toolchain; silent fall-through would unpin you.

## Open questions (to discharge before this leaves fiction)

- Validate the `@tag` spellings against a real facet-styx decode of the Rust
  `Input` enum (unit variants like `@exit-only`/`@neutral` included), and
  publish the schema via `GenerateSchema` so styx-lsp checks manifests.
- `program rustc` → capability type `Rustc`: name-case mapping rule, or an
  explicit `capability Rustc` field? (Currently: implicit Pascal-casing.)
- Cross-package imports (`use cargo::…` keyed by input name) are new module
  doctrine — today's module graph is directory-local.
- Command argument tails (`vx r facet test --filter …`): trailing `[String]`
  first, the parsed-but-unlowered `command` grammar items later.
- Manifest-named fns as roots (beside `#[test]`) need compiler standing.
- If vix programs ever read manifests themselves (registry tooling in vix),
  the missing `Map<String, _>` decode target bites; the repeated-node
  spelling (`command build {…}` not `commands { build {…} }`) avoids it.
