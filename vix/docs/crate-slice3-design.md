# Crate Slice 3 Design: Build Scripts and Proc-Macros

Design pass only — no implementation. Slice 1 (single crate, fake+real rustc)
and slice 2 (deps, `--extern`/`-L dependency`, rmeta split, real-rustc oracle,
PR #2440) establish the shape this slice extends: `assign_roles`/`tool_for` in
`vix/src/lib.rs` hand-lower a command name + argv into an `ExecPlan`; the
two-tier `ExecCache` in `vix/src/exec.rs` runs or reuses it; `RealProcessBackend`
in `vix/src/real_process.rs` stages declared roles into a tempdir and shells
out for real. Capabilities (`Cc`/`Ar`/`Rustc`) are acquired per `Target` and
journaled as `acquire:{kind}:{target_hash}` (`vix/src/machine/lower.rs:3879`,
`driver.rs:5651`) — a capability's identity is already `(kind, target)`, which
matters below.

Cargo remains an oracle, never a dependency (`cargo-manifest-build.md`): the
production path never invokes it; `cargo +nightly build --unit-graph
-Z unstable-options` is the only place its output feeds a *test*.

This slice has two seams. Both are cases of "one exec's output becomes the
next exec's input flags," which nothing in slices 1–2 needed: every prior
input was static source/manifest data, and every producer fed a consumer
through `--extern`/`-L` file paths only.

## Seam 1: Build Scripts

### What Cargo does

A `custom-build` unit in `mode: "build"` compiles `build.rs` — a completely
ordinary host-platform `rustc` invocation, `--crate-type bin`. A second
`custom-build` unit in `mode: "run-custom-build"` depends on that binary and
represents *running* it: Cargo execs the binary with `OUT_DIR`,
`CARGO_MANIFEST_DIR`, `CARGO_PKG_*`, `TARGET`, `HOST`, `OPT_LEVEL`, `PROFILE`
set, captures stdout, and:

- parses `cargo:rustc-cfg=…`, `cargo:rustc-link-lib=…`,
  `cargo:rustc-link-search=…`, `cargo:rustc-env=…`, `cargo:warning=…` lines
  into flags for the *parent* package's rustc invocation;
- keeps whatever files the script wrote under `OUT_DIR` — shape unknown ahead
  of time, since the script can write any file it likes there (vix's own
  `build.rs` is a live example: it writes `vix_ast.rs`/`vix_grammar.json` into
  `OUT_DIR`, which `src/lib.rs` reads back with `include!(concat!(env!("OUT_DIR"), …))`).

Two things cross a boundary that slices 1–2 never had: parsed **directives**
(structured data extracted from stdout) and an **unpredictable file tree**
(`OUT_DIR`). Both become inputs to a third, ordinary rustc invocation (the
parent crate's compile).

### Machine shape

Three nodes, chained by demand:

1. **Compile build.rs.** Nothing new — an ordinary `rustc!` lib-turned-bin
   compile, same `assign_roles`/`FakeRustc`/`RealProcessTool` path slice 2
   already has, acquired against a `Rustc` capability. (Which `Target` it
   acquires against is the seam-2 question below — build scripts run on the
   HOST, so this should already be `Rustc::acquire(host_target)`, not the
   package's target.)

2. **Run build.rs.** A genuinely new exec shape: the "tool" is not `cc`/`ar`/
   `rustc` but the *produced binary itself*, invoked with an environment map
   instead of flag argv, whose primary product is stdout (not a declared
   output file) plus an out-of-band directory tree. Concretely, this needs:
   - a new command grammar (`assign_roles`/`tool_for` entry) for "run a staged
     executable with env vars" — argv is empty or minimal, the acquired
     capability is the build-script binary handle itself rather than a
     toolchain, and role assignment is dominated by env-map handling that
     doesn't exist in `ExecPlan`/`Role` today;
   - stdout capture as a first-class artifact. `Tool::run` returns `Tree`;
     nothing captures stdout today (`RealProcessTool` discards it on success).
     The plan: harvest stdout into a well-known synthetic tree entry (e.g.
     `$stdout`) so `Outcome`/`ExecCache`/tier-2 verification don't need new
     shapes — only the harvesting code changes;
   - `OUT_DIR` as an **unpredictable output tree**, not a declared output
     file. `real_process.rs::harvest_outputs` only reads paths that appeared
     as `Role::Output` argv entries; it has no "walk this directory and
     harvest everything under it" mode. This wants a new `Role` (e.g.
     `Role::OutputDir`) recognized by `prepare_output_dirs`/`harvest_outputs`,
     harvested by directory walk after the process exits. Tier-2 read-set
     semantics (`verify()`) are unaffected — they cover reads, not writes —
     but this is still a genuine `exec.rs` primitive addition, not a lowering
     detail.

   This node's cache identity is exactly today's two-tier shape: tier 1 keys
   on (build-script binary content hash, env map, declared read-set ceiling);
   tier 2 candidates replay if the observed read-set (files/env the script
   actually touched, in the open real-process lane: *declared roles only* —
   arbitrary host reads are not intercepted, same honest limitation
   `cargo-manifest-build.md` already states for `cc`/`rustc`) still verifies.
   Nothing about the two-tier design changes; the read-set for this node is
   just smaller in practice because build scripts read `env::var` and
   arbitrary paths that no grammar declares.

3. **The directive parse.** Pure and cacheable — a **probe**, exactly like
   `toml()`/`json()`/`ast()`/`oci()`: `build_directives(stdout: String) -> Doc`
   (or a dedicated struct) turning `cargo:key=value` lines into a typed value.
   No exec, no capability, no cache tier of its own beyond ordinary memoization
   — this is the cheap part.

The parent crate's `rustc!` block then demands: `build_directives(...).cfgs`
lowered to `--cfg` flags, `.link_args` lowered to `-l`/`-L` flags, and (when
the source does `include!(concat!(env!("OUT_DIR"), "/x.rs")))` the OUT_DIR
tree mounted so that path resolves as an ordinary rustc `Input` read. The
demand edge is exactly the prompt's framing: build-script-output →
parent-compile-flags, materialized as one probe result feeding into another
unit's `ExecPlan`.

### Oracle

`cargo +nightly build --unit-graph`: compare unit count/shape — one `build`
unit, one `run-custom-build` unit, correct dependency edge from the parent
unit to the `run-custom-build` unit. Beyond the unit graph (which doesn't show
directive *content*), the artifact oracle has to do the real work here: a real
`cargo build` vs a real vix build of a fixture whose behavior differs
depending on whether the cfg/env from the directive was actually threaded
through — a shape mismatch in the unit graph proves lowering is structurally
sane, but only a differing-behavior artifact proves the directive→flag wiring
is real (a build with no cfg set silently "passing" a diff on absent files
would be a false success).

### Mechanical vs. new capability vs. Amos questions

- **Mechanical (reuse slice 1/2 machinery as-is):** compiling build.rs is an
  ordinary `rustc!` invocation; the directive parse is an ordinary probe like
  `toml()`.
- **New machine capability (exec.rs primitives, small and general):**
  stdout-as-artifact harvesting; a directory-shaped output role
  (`Role::OutputDir` or equivalent) with directory-walk harvest.
- **Design questions for Amos:**
  1. Does "run a staged binary with an env map" belong in the `foo! { }`
     command-block surface (`grammar.js`'s token-soup-plus-splices shape
     doesn't fit env vars naturally), or does it want a dedicated typed
     builtin the way `Cc::acquire`/`toml()` are builtins rather than command
     blocks?
  2. Should build-script *running* be real-process-lane-only, never
     fake-VFS — since it's unsandboxed arbitrary host code by nature, unlike
     `cc`/`rustc` where a fake tool is a meaningful test double? (Leaning:
     yes; a fake build-script tool that emits scripted directives is still
     useful for machine/cache tests, so probably both, but the *reference*
     behavior can only ever be defined by the real process.)
  3. `Role::OutputDir` harvest semantics: does a later run that removes a file
     from `OUT_DIR` need to be observable (tier-2 style), or is
     "overwrite/add, never diff away stale files" acceptable for v0 given the
     open lane already can't see everything a script touches?

## Seam 2: Proc-Macros

### What Cargo does

A proc-macro crate is a `lib` unit with `target.kind` and `target.crate_types`
both `["proc-macro"]`, compiled for the **host**, producing a dylib. A
dependent target-platform unit loads that dylib via `--extern
name=/path/to/libfoo.so` — mechanically identical to any other `--extern`
edge slice 2 already wires, except the producing unit's rustc runs against a
different platform than the consuming unit's.

### Machine shape

This is mostly reuse, not a new machine primitive — the hard part is a
**capability/lowering decision**, not new exec plumbing:

- The producing unit acquires `Rustc::acquire(host_target)`; the consuming
  unit acquires `Rustc::acquire(target)`. Capability identity is already
  `(kind, target_hash)` (`acquire:{kind}:{target_hash}`, `driver.rs:5651`),
  so host and target Rustc are automatically distinct capabilities, distinct
  cache entries, distinct journal pins — nothing new needed there. `Target`
  today (`driver.rs:1546`) only encodes `os`; it has no explicit "this is the
  host" bit distinct from a real cross-target triple. On a dev box where
  host == target, the distinction is invisible unless the lowering code is
  disciplined about which `Target` value it acquires against for which unit
  kind — which is exactly the bug this design has to prevent, since it won't
  show up as a failure until someone actually cross-compiles.
- The produced dylib is a `Tree` output exactly like any other rustc artifact;
  it crosses into the dependent unit's `ExecPlan` via the same `--extern`
  wiring slice 2 built. `--crate-type proc-macro` is a flag, not a new role.
- Build order: proc-macro units must finish (their dylib exists) before any
  unit that `--extern`s them — the same producer/consumer demand edge as any
  other dependency, no new ordering primitive.

### Oracle

`cargo +nightly build --unit-graph`: compare `target.kind` /
`target.crate_types` == `["proc-macro"]` on the producing unit, and — the
part that actually matters for this seam — that the producing unit's rustc
invocation carries no (or the host) `--target` while sibling units carry the
package's real target. Artifact oracle: a consumer whose behavior only makes
sense if the macro actually expanded (e.g. a generated `const`/`fn` the
consumer's `main` prints), so a same-source-passthrough bug can't silently
pass.

### Mechanical vs. new capability vs. Amos questions

- **Mechanical:** the entire `--extern`/rmeta wiring, `--crate-type
  proc-macro` as a flag, the producer→consumer demand edge.
- **New machine capability:** none required by the happy path. Arguably
  none *at all* if slice 3 fixtures stay same-host — the design still has
  to get the host-vs-target `Target` selection right so cross-compilation
  isn't a rewrite later.
- **Design question for Amos:** should `Target` grow an explicit host/cross
  distinction now (e.g. a `Target::host()` constructor distinct from
  whatever `Target` the package build is targeting, even when they're
  presently equal), or is deferring that until an actual cross-compiling
  fixture exists the right amount of "don't build for hypothetical
  requirements" here? I lean toward adding `Target::host()` now since it's
  the one-line difference between "this seam is proven" and "this seam
  happens to work because host == target in every fixture we wrote" — but
  it's a real call either way.

## Slice Plan

Two slices, build scripts first:

- **3a — build scripts.** New exec.rs primitives (stdout-as-artifact,
  directory-output harvest) are the actual novelty in this design; proving
  them first means slice 3b is closer to pure reuse. Minimal real fixture:
  a package with a `build.rs` that does exactly
  `println!("cargo:rustc-cfg=vix_slice3");` and a `lib.rs` whose behavior
  differs under `#[cfg(vix_slice3)]` (so the artifact oracle can catch a
  wiring bug, not just a "file exists" bug). A second fixture step — `build.rs`
  writing one file into `OUT_DIR` consumed via
  `include!(concat!(env!("OUT_DIR"), "/generated.rs")))` — proves the
  directory-output path using the exact pattern vix's own `build.rs` already
  depends on.
- **3b — proc-macros.** Minimal real fixture: a proc-macro crate exporting one
  trivial `#[proc_macro] pub fn` (or function-like macro) that emits a fixed
  token stream, and a consumer crate whose `main` uses the macro's output in a
  way that shows up in the built binary's behavior. No build script, no
  dependency depth beyond the two crates, so this slice isolates the
  host/target capability-selection question from seam 1 entirely.

Order is a recommendation, not a given — flagged as an open question above
since proc-macros could equally go first as the more contained, mostly-reuse
slice.
