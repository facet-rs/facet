# Cargo Manifest Build Lane

This lane lowers a resolved Cargo package unit into vix exec commands. Version
resolution is out of scope: the input is a manifest source tree plus an already
chosen package/dependency/feature set.

Cargo is an oracle, never a dependency. Production code in this lane never
invokes Cargo for metadata, manifest parsing, source discovery, plan derivation,
or building. The production path parses `Cargo.toml` through `toml()`, applies
Cargo's documented target/source conventions directly, derives rustc flags
itself, and executes through the vix exec seam. Cargo appears only in test
helpers with `oracle` in the name:

- `cargo +nightly build --unit-graph -Z unstable-options`, compared against the
  derived vix plan;
- cargo-built artifacts, compared against vix-built artifacts in end-to-end
  artifact tests.

If production code needs Cargo "temporarily", the lane stops instead.

## Cargo's Oracle

Cargo nightly exposes the differential target:

```text
cargo +nightly build --unit-graph -Z unstable-options --manifest-path <Cargo.toml>
```

For a no-dependency library, the JSON is:

```text
{
  "version": 1,
  "units": [
    {
      "pkg_id": "path+file:///...#unit_graph_probe@0.1.0",
      "target": {
        "kind": ["lib"],
        "crate_types": ["lib"],
        "name": "unit_graph_probe",
        "src_path": ".../src/lib.rs",
        "edition": "2021",
        "doc": true,
        "doctest": true,
        "test": true
      },
      "profile": {
        "name": "dev",
        "opt_level": "0",
        "debuginfo": 2,
        "debug_assertions": true,
        "overflow_checks": true,
        "incremental": true,
        "panic": "unwind"
      },
      "platform": null,
      "mode": "build",
      "features": [],
      "dependencies": []
    }
  ],
  "roots": [0]
}
```

With path deps, a build script, a feature, and a proc macro, cargo adds:

- one root `bin` unit whose `dependencies` list edges by unit index and
  `extern_crate_name`;
- one `custom-build` unit in `mode: "build"` that compiles `build.rs`;
- one `custom-build` unit in `mode: "run-custom-build"` that depends on the
  compiled build script and represents running it;
- ordinary `lib` dependencies with their activated `features`;
- `proc-macro` units with `target.kind` and `target.crate_types` both set to
  `["proc-macro"]`.

The vix lowering should emit a plan graph with the same unit identities, edges,
target paths, features, and profile-derived flags as this JSON. Tests should
compare against cargo's unit graph before comparing artifacts.

## Manifest To Rustc Inputs

Cargo derives each rustc invocation from a package, a target, a feature set,
profile settings, and resolved dependency artifacts:

- crate name: `package.name`, normalized from `-` to `_`, unless a target
  overrides `name`;
- edition: `package.edition`, defaulting through Cargo's package defaults;
- crate type: `lib` defaults to `lib`; `[[bin]]` defaults to `bin`;
  `[lib].crate-type` supplies explicit crate types;
- source discovery: `[lib].path` or `src/lib.rs`; `[[bin]].path`, otherwise
  `src/main.rs` for the implicit binary or `src/bin/<name>.rs` for listed bins;
- output flags: `--crate-name`, `--edition`, `--crate-type`, input path,
  `--out-dir`, and dev-profile codegen/debug/assertion flags;
- features: the chosen feature set is an input to this lane and lowers to
  `--cfg feature="<name>"` plus Cargo's package feature environment where
  needed later;
- dependencies: each resolved dependency unit contributes
  `--extern <extern_crate_name>=<artifact-path>` after that unit has produced an
  rlib, dylib, proc-macro dylib, or rmeta;
- search paths: dependency artifact directories lower to `-L dependency=<dir>`;
- build scripts: `build.rs` is compiled as a host unit, then run with Cargo's
  build-script environment, including `OUT_DIR`; its stdout lines such as
  `cargo:rustc-cfg=...`, `cargo:rustc-link-lib=...`, and
  `cargo:rustc-link-search=...` feed the parent package units;
- proc macros: proc-macro crates are host units and their artifacts are passed to
  target units through `--extern`, independent of the target unit's platform.

## Slice 1

Slice 1 is deliberately narrower than Cargo:

- one package source tree, already present as a vix `Tree`;
- no dependencies, no build script, no proc macros;
- implicit library target at `src/lib.rs` and optional implicit binary at
  `src/main.rs`;
- `package.name` and `package.edition` read with `toml()`;
- dev profile only;
- `rustc!` command vocabulary with roles for inputs, outputs, and search dirs;
- fake-VFS exec tests proving cold run, warm memo hit, and tier-2 cutoff after an
  unread file edit.

The default exec substrate is fake-VFS-only: command tools are in-process
implementations behind `Tool`, and this remains the CI/default lane. The
`real-process` Cargo feature adds an opt-in native backend that runs the same
command-role plans as host processes. It is deliberately unsandboxed and trusts
the host: vix stages role-declared inputs into a temporary work directory,
scrubs the environment to an explicit allowlist, runs the host command, and
harvests declared outputs back into vix trees.

That open backend is not Vixen's sound runtime lane. It has no VFS
interception, no syscall mediation, and no sandbox ceiling beyond what staging
happens to provide. Tier 2 verifies only what command roles declare: input-role
content bytes and search-dir membership. Header contents discovered by a real
compiler through the host filesystem are not observed unless the command grammar
declares them as inputs. This is enough for local artifact smoke tests and for
the cache behavior that open vix can honestly own; proprietary VFS mediation is
the sound lane.

Slice 1 can prove lowering and cache behavior with the fake `rustc` tool, and
with `--features real-process` it can additionally compile trivial host
artifacts through `cc!`. The end-to-end real `cargo build` artifact comparison
still waits for slice 2's real `rustc` plan graph.

## Later Slices

Slice 2 should add a real `rustc` oracle on top of the same real-process
staging: path dependencies and `--extern`/`-L dependency` wiring, then split
rmeta and final artifact units so dependents can start from metadata where
rustc permits it. It will need command grammars that declare every rustc source,
extern artifact, search path, response file, and emitted artifact explicitly;
otherwise open tier-2 verification can only prove the subset the roles name.
After that, add build-script compile/run units and feed their declared outputs
into parent rustc invocations. Proc-macro host units can share most dependency
wiring but need host/target separation in the plan keys.

## Lockfile Consumer Seam

The resolution-to-build interface is a Cargo.lock-shaped resolved manifest.
Open `crate.vix` consumes it as data with `toml()`. It does not invoke Cargo to
resolve, inspect metadata, or build.

The current open consumer accepts this Cargo.lock subset:

- top-level `version` is allowed and ignored by the builder;
- `[[package]]` tables are the resolved package set;
- each package must provide `name` and `version`;
- `dependencies = ["dep_name", ...]` is the resolved direct dependency list by
  package name;
- `source` is accepted when present, with Cargo's normal string shape, but path
  packages may omit it exactly as Cargo.lock does.

For the vendored offline fixture, package source trees live beside the lockfile:
`app/` for the root binary and `crates/<package-name>/` for libraries. That
layout convention is the open fixture's source locator; the lockfile remains the
resolved package/edge contract.

The build graph is demand-shaped rather than Cargo-shaped. A package's metadata
or link artifact demands its direct dependencies' rmeta/rlib trees, and rustc
receives those artifacts through `--extern` plus `-L dependency` search paths.
The current fixture proves a transitive graph:

```text
mini_app -> alpha_lib -> core_lib
mini_app -> formatting_lib
```

The default fake lane proves the command graph and extern wiring. With
`real-process`, the same vix entry points run real `rustc`, execute the produced
binary, and compare the derived unit shapes with
`cargo +nightly build --unit-graph -Z unstable-options --locked` as the oracle.

## Rodin Emit-Lock Follow-Up

Rodin stays a separate proprietary resolver lane. The remaining bridge is small:
convert Rodin's `SolveResult` into the Cargo.lock-shaped document above, including
the selected package rows and direct dependency names. Once Rodin emits that
lock-shaped tree, the open `crate.vix` consumer can build it without calling
Cargo, closing the resolve -> emit-lock -> build round trip.
