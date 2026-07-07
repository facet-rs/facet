# Monorepo oracle recon — when can vixen build the whole facet monorepo?

Recon only. Nothing in `vix/src`, `weavy/src`, or `rodin/*.vix` was touched.
This is a measurement pass against two oracles: cargo itself run on the real
facet-cc workspace, and cargo itself run on its own upstream repo (added by
Amos mid-session as a second, structurally different oracle). All numbers
below come from actual `cargo`/`grep`/`nextest` invocations, not estimates.

## TL;DR

Vixen cannot build the facet monorepo yet, at any tier, and the gap is not a
long tail of small bugs — it's that the "generic build walk" landed on
`origin/rodin` is generic in name only. It is a hand-written dispatcher over
**named** crates (`blake3`, `autocfg`, `taxon`, `facet-core`, `facet-macros`)
and dependency-count arities (0, 1, 2, "dynamic"), validated against fixtures
with 2-4 packages. The moment it sees a manifest outside that fixture set —
which is all 863 packages of the real workspace — it either mis-derives a
value silently or panics on an `.unwrap()`. Separately, **no profile flag is
emitted at all** (not even for the single "dev profile only" Slice-1 goal):
zero occurrences of `opt-level`/`debug-assertions`/`overflow-checks`/`panic`/
`codegen-units` anywhere in `vix/src`. Rodin's resolver (the 40-search fold)
covers version search/backtracking against 4-5 hand-built fixtures; feature
resolution (rodin doc `60-features.md`) and cfg/target gating (`70-targets-cfg.md`)
are **distilled to prose but not yet implemented natively** — the current
resolver skips the one fixture that needs weak-optional-feature activation.

Second oracle (upstream `rust-lang/cargo`, cloned this session) sharpens the
picture instead of complicating it: cargo's own manifest surface is *cleaner*
for Tier A (no hand-rolled per-name hardcoding could survive contact with it
either, but the surface itself — globs, a mixed workspace+package root
manifest — is orthogonal complexity, not more of the same complexity) but its
build closure is a completely different animal for Tier B: 13 `-sys` crates
whose `build.rs` shell out to `pkg-config`, `curl-config`, and `git`, and
conditionally compile vendored C via the `cc` crate. Facet-cc's own closure
has exactly one such crate (`aws-lc-sys`, and it isn't even reachable in the
current resolve — see §5). **The working hypothesis is CONFIRMED**: cargo is
comparatively easier for Tier A and dramatically harder for Tier B.

## 1. Methodology

- Oracle A ("facet-cc"): `~/oss/facet-cc` main checkout (not this worktree —
  this worktree is a few commits ahead on `monorepo-oracle-recon`, so the
  oracle captures use the canonical tree). Commit at capture time: whatever
  `origin/rodin`-derived HEAD was checked out there; `Cargo.lock` used as-is
  (`--locked`).
- Oracle B ("cargo-oracle"): `git clone --depth 1 https://github.com/rust-lang/cargo`
  into `/tmp/monorepo-oracle-recon/cargo-oracle`, pinned at
  `28fa7f2a3d8cbc62256aaf0d8464f9ecc96248ff` (2026-07-06). Shallow clone,
  read-only, not committed anywhere.
- `cargo metadata --format-version 1` and, where nightly was available (it
  was, `nightly-aarch64-apple-darwin` is installed on this host),
  `cargo +nightly build --unit-graph -Z unstable-options --workspace` for
  both oracles. Both succeeded without needing to install anything.
- Vix-side evidence is **static reading of the production `.vix`/Rust
  source**, cross-checked against `cargo nextest run -p vix --features
  real-process` (168/168 passed, 2 skipped — see §4) and against the fixture
  files already checked into the repo (`playgrounds/snark/.../fixtures/cargo_manifest_real/`)
  which are byte-identical copies of a handful of real crates, so gaps cited
  below are pinned to exact file:line evidence, not hypothesized.
- Raw captures (cargo metadata JSON, unit-graph JSON, stderr logs) are left
  in `/tmp/monorepo-oracle-recon/` — not committed (large, regenerable in
  seconds, see commands above).

## 2. Oracle stats, side by side

| | facet-cc (real workspace) | cargo (upstream, rust-lang/cargo) |
|---|---|---|
| `cargo metadata` packages | 863 | 501 |
| workspace members | 146 (explicit paths) | 27 (2 globs: `crates/*`, `credential/*`) |
| unit-graph units (`--workspace`, dev profile) | 880 (804 build + 76 run-custom-build) | 528 (482 build + 46 run-custom-build) |
| unit-graph roots | 155 | 30 |
| lib / custom-build / proc-macro / bin units | 652 / 152 / 48 / 19 | 406 / 92 / 19 / 11 |
| non-`lib`/`bin` crate-types | `cdylib`+`rlib`/`staticlib` in styx-wasm, styx-ffi, snark-wasm, vox-inprocess, subject-rust, wasm-*-tests (10 crates) | none |
| resolver | `"3"` | `"2"` |
| `[workspace.dependencies]` entries | ~190 | 109 |
| member manifests using `dep.workspace = true` / `{ workspace = true }` | 1356 grep hits | 43 files |
| `[target.'cfg(...)'.*]` tables (real, non-test-fixture manifests) | 21 | 8 (root + 6 crate manifests; the other ~25 hits are cargo's own `tests/testsuite/**` fixture Cargo.tomls, not real build inputs) |
| cfg-gated dependency edges (`cargo metadata` resolve, `dep_kinds[].target` set) | 299 / 2909 edges | 163 / 1548 edges |
| `build.rs` present | 12 crates | 4 (root, `cargo-test-support`, `tests/testsuite`, `build-rs-test-lib`) |
| `proc-macro = true` crates | 10 | 1 |
| `[[bin]]` / `[[example]]` / `[[test]]` / `[[bench]]` tables | 21 / 5 / 10 / 13 | 61 / 0 / 0 / 3 |
| `[dev-dependencies]` / `[build-dependencies]` sections | 83 / 17 | 63 / 39 |
| `[features]` sections | 61 | 103 |
| `optional = true` deps | 175 | 56 |
| git dependencies (real manifests) | 0 (only in the `tools/` excluded sub-workspace) | 0 (only in `tests/testsuite/**` fixture outputs, i.e. test data, not real deps) |
| `[patch.crates-io]` entries | 85 | 0 |
| root-manifest `[profile.*]` overrides | `[profile.profiling]`, `[profile.fuzz]`, `[profile.dev.package.backtrace]` (+13 more `[profile.dev.package.*]` on this branch) | none |
| root manifest shape | pure virtual workspace manifest (`[workspace]` only) | **mixed**: `[workspace]` + `[package]` (name="cargo") in the same file |
| `-sys` crates in the dependency closure | 16 (see §5) | 13 (see §5) |

Neither workspace uses member globs except cargo itself. Neither uses git
dependencies in its real build graph. Facet-cc is the one with heavier
`[patch]` and per-package profile-override usage; cargo is the one with the
mixed root manifest and actual glob member expansion.

## 3. What the existing vix machinery actually covers

Two production `.vix` files carry the whole "cargo build lane":
`playgrounds/snark/src/bundled/vix/samples/cargo_manifest.vix` (272 lines,
manifest → shape) and `.../samples/crate.vix` (816 lines, shape → rustc
invocations + the "generic build walk" `ResolvedGraph` walker praised in
`RESURRECTION.md` as matching cargo's unit graph). `vix/docs/content/cargo-manifest-build.md`
(gitignored, main checkout only) is honest about scope: "Slice 1... one
package source tree... no dependencies, no build script, no proc macros...
dev profile only." That doc is accurate — what's undocumented is *how* the
later slices (real deps, build scripts, proc macros) were actually made to
pass: by naming the exact crates in the fixture, not by generalizing the
manifest reader.

Concrete evidence, `cargo_manifest.vix`:

```
fn dependency_is_workspace(name: String) -> Bool {
    match name == "blake3" {
        true => true,
        false => name == "autocfg",
    }
}

fn workspace_dependency_path(workspace: Tree, name: String) -> String {
    match name == "taxon" {
        true => workspace_dependency_doc(workspace, name).get("path").unwrap(),
        false => "",
    }
}

fn dependency_default_features(dep: Doc, name: String) -> Bool {
    match name == "blake3" {
        true => dep.get("default-features").unwrap(),
        false => match name == "facet-core" {
            true => dep.get("default-features").unwrap(),
            false => match name == "facet-macros" {
                true => dep.get("default-features").unwrap(),
                false => true,
            },
        },
    }
}
```

This is not "generic with gaps" — it is a **name allowlist** standing in for
what should be "read the `workspace = true` key off the dependency table."
The fixture that backs the passing tests
(`playgrounds/snark/.../fixtures/cargo_manifest_real/facet/Cargo.toml`) is a
byte-identical copy of the real `facet/Cargo.toml`, and it already contains
the disproof, unexercised:

```
# facet/Cargo.toml:136, :146 — never touched by any cargo_manifest.vix test
static_assertions = { workspace = true, optional = true }
tempfile = { workspace = true }
```

`dependency_is_workspace("static_assertions")` returns `false` today, so
`dependency_version_req`/`dependency_path` fall into the non-workspace branch
and call `doc_string(dep, "version")` / `doc_string(dep, "path")` — `.get("version").unwrap()`
on a table whose only keys are `workspace`/`optional`. That is a real,
already-checked-in reproduction of the panic, not a hypothetical: run
`detailed_dependency_of` against `facet`/`"static_assertions"`/`"normal"`
through the existing `manifest_machine()` harness in `vix/tests/cargo_manifest.rs`
and it unwraps `None`.

`crate.vix`'s "generic build walk" (`generic_lib`, `generic_bin`, …) is
arity-dispatched, not degree-generic:

```
fn generic_lib_with_deps(...) -> Tree {
    match unit.deps.len() == 1 {
        true => generic_lib_with_one_dep(...),
        false => generic_lib_with_dynamic_deps(...),
    }
}
```
```
fn generic_bin_check_with_deps(...) -> Tree {
    match unit.deps.len() == 1 {
        true => ...with_one_dep(...),
        false => match unit.deps.len() == 2 {
            true => ...with_two_deps(...),
            false => ...with_dynamic_deps(...),
        },
    }
}
```

The 0/1/2/dynamic split exists because the language currently has no
general "fold over N extern args" primitive that both the tests and the
language docs consider clean yet (`target_shapes_array_gap()` and
`resolved_unit_adaptation_gap()` in `cargo_manifest.vix` are the authors'
own pinned admissions: no `[CargoTargetShape]` array return, no
string-to-`Path` constructor). The `_dynamic_deps` arm does generalize via
`direct_extern_args`/`dependency_artifacts` recursion, so N-ary fan-out
*is* handled — the arity split is a perf/readability artifact, not a hard
wall. It is called out here because it's evidence the walker has only ever
been exercised at N ∈ {0,1,2}: the two- and four-package fixtures.

Profile flags: **zero** occurrences of `opt-level`, `debug-assertions`,
`overflow-checks`, `panic=unwind`, `codegen-units`, or `debug_assertions`
anywhere in `vix/src` (grepped directly). The command classifier in
`vix/src/lib.rs` (`"rustc" => { ... }`) only assigns *roles* (Input/Output/
SearchDirFlag/InputFlag/Env/Flag) to whatever argv the `.vix` script already
built — it does not synthesize any flags itself. Since no `rustc!` call site
in `crate.vix` emits a profile flag either, **the "dev profile only" goal
stated in Slice 1 is not met**: no dev-profile flag is emitted, so nothing
distinguishes it from any other profile. `[profile.dev.package.backtrace]`
and friends (14 per-package overrides on this branch, 3 custom named
profiles at the root) have no code path at all.

Rodin: `rodin/PLAN.md` states plainly that features (`60-features.md`) and
cfg/target gating (`70-targets-cfg.md`) are **distilled to prose, not
implemented** — "Next: review the distillation, then delete `rodin-core`...
then implement native from `docs/` + cargo." `vix/tests/rodin_fixtures.rs`
confirms this operationally: of 5 differential fixtures,
`cfg_any_and_weak_feature_never_pull_optional_dep` is `#[ignore = "pending
optional weak feature activation fix in rodin.vix"]`. The other 4 pass, but
they're `direct_target_conditional_edge`, `feature_activated_target_conditional_optional_dep`,
`build_dependency_is_consumed`, `transitive_dev_dependency_is_not_consumed`
— one edge each, not a resolve over hundreds of crates with hundreds of
optional/default features layered across normal/build/dev scopes (facet-cc:
175 `optional = true` deps, 61 `[features]` sections, 190 workspace deps).

## 4. Empirical baseline (ran the suite, didn't just read it)

```
cargo nextest run -p vix --features real-process
168 tests run: 168 passed (2 slow), 2 skipped   [185.6s test time / 3m53s wall]
```

The 2 skipped are the ignored weak-feature-activation fixture (rodin) and
its pairing. Every green test operates on a fixture of 2-8 hand-picked
crates (`two_crate_graph`, `lock_graph` = mini_app/alpha_lib/formatting_lib/core_lib,
`proc_macro_graph`, `cargo_manifest_real` = taxon/facet-core/facet + 5 more
copied but unused-by-tests crates). Nothing in the suite touches more than
~8 packages at once; nothing touches the 146-member root manifest as it
actually exists (the fixture root manifest hand-trims `members` down to 3
entries — see §1 evidence). No scaling test exists to fail loudly; the gaps
above only show up the moment you point the same functions at manifests
outside the allowlist, which nothing in CI currently does.

## 5. Tier B: the hermeticity boss fight, both oracles

Per Amos's ask: name and characterize the native-toolchain closure on both
sides.

**cargo (upstream), 13 `-sys` crates** in its own `cargo metadata` closure
(macOS aarch64 host): `curl-sys`, `libgit2-sys`, `libssh2-sys`, `libz-sys`,
`libnghttp2-sys`, `libsqlite3-sys`, `core-foundation-sys`,
`security-framework-sys`, `js-sys`, `web-sys`, `windows-sys`,
`linux-raw-sys`. (`openssl-sys` is in `Cargo.toml` as a workspace dep but did
not resolve on this platform — macOS TLS goes through `security-framework`
instead; it would resolve on Linux.) Their `build.rs` files (read directly
from `~/.cargo/registry/src/...`) all do real native-toolchain probing:

- `curl-sys/build.rs` (599 lines): `Command::new("curl-config")`,
  `Command::new("git")` (submodule init for vendored curl), `cc::Build`,
  `pkg_config::probe_library`, feature-gated vendored-vs-system dispatch.
- `libgit2-sys/build.rs` (362 lines): same shape — `pkg_config::`, `cc::Build`,
  reads `CARGO_FEATURE_VENDORED`/`CARGO_FEATURE_HTTPS`/`CARGO_FEATURE_SSH`/
  `CARGO_FEATURE_ZLIB_NG_COMPAT` to decide what to vendor vs. link.
- `libssh2-sys`, `libz-sys`, `libnghttp2-sys`, `libsqlite3-sys`: same
  `pkg_config`/`cc::Build` pattern; `libsqlite3-sys` additionally reads
  `DEP_OPENSSL_INCLUDE` (cross-crate `links=` metadata) and half a dozen
  `SQLITE_MAX_*` env vars.
- `cargo/Cargo.toml` itself declares `vendored-openssl`, `vendored-libgit2`,
  and an `all-static` feature union — i.e. cargo's own manifest already
  encodes "the vendored-vs-system decision is a build-time feature flag,"
  which is exactly the kind of non-declarative, env/probe-driven build
  surface vix's command-role model (`cargo-manifest-build.md`: "Header
  contents discovered by a real compiler through the host filesystem are
  not observed unless the command grammar declares them as inputs") is not
  designed to observe soundly. This is the hermeticity boss fight in one
  sentence: cargo's own dependency closure decides its build shape by
  running `pkg-config`/`curl-config`/`git` and reading ambient env, not by
  reading declared manifest data.

**facet-cc, by contrast: 1 comparable crate.** Its own `build.rs` set (12
files) is almost entirely small Rust-native codegen (`facet-core/build.rs`
20 lines, `facet/build.rs` 20 lines, `facet-hash/build.rs` 158 lines,
`facet-json/build.rs` 108 lines — none touch `cc::Build`/`pkg_config`/
`Command::new`). The single exception is `snark-scanner-host/build.rs`,
which does use `cc::Build` + `Command::new`. Its `Cargo.lock` (893 packages)
lists 16 `-sys` crates, but 15 of them are thin FFI-binding crates with no C
compilation step (`windows-sys`×4, `linux-raw-sys`×2, `*-sys` file-watcher
bindings `fsevent-sys`/`inotify-sys`/`kqueue-sys`, macOS framework bindings
`core-foundation-sys`/`security-framework-sys`, `js-sys`/`web-sys`,
`dirs-sys`) plus `libsqlite3-sys` (used via `rusqlite = { features =
["bundled"] }` — compiles vendored SQLite via `cc`, but with **no**
`pkg-config`/env probing, just "run a C compiler over a vendored .c file").
The one real Tier-B risk is **`aws-lc-sys`** (pulled transitively by
`rustls`/`rustls-webpki`, present in `Cargo.lock`), whose `build.rs`
(checked directly in the registry cache) shells out to `cmake`, `nasm`,
`go`, and `bindgen` — a materially harder native closure than anything else
in facet-cc. **It is currently latent, not live**: it has zero nodes in
`cargo metadata`'s resolved dependency graph for this workspace/platform
(`resolve.nodes` — grepped, zero matches for `aws-lc-rs`/`aws-lc-sys`), so
it's a leftover/alternate-feature-combination entry in the V3 lockfile, not
something `cargo check --workspace --all-targets` actually builds today on
this host. It would become live the moment any workspace member's TLS
feature selection changes, so it's worth flagging even though it doesn't
block the claim today.

**Verdict on the working hypothesis: CONFIRMED.** Cargo's own manifest
surface (globs, mixed root manifest) is different-shaped complexity for
Tier A, not obviously *harder* — the vix ingestion layer would need general
glob expansion and "a manifest file can declare both `[workspace]` and
`[package]`" either way, and neither is a bigger lift than the workspace-
inheritance-shorthand gap already blocking facet-cc. But for Tier B, cargo's
closure requires probing/spawning external toolchain discovery
(`pkg-config`, `curl-config`, `git`) inside `build.rs` — a fundamentally
different problem from "run a declared `rustc!`/`cc!` command role," and
categorically harder than facet-cc's closure, where the one comparable case
(`aws-lc-sys`) isn't even reachable today.

## 6. Gap catalog, ranked by blast radius

Tier legend: **A** = resolve+plan matches cargo's unit graph/lockfile; **B**
= hermetic check/build (rmeta level, build.rs protocol, proc-macro host
builds, `--extern` splicing); **C** = full link, all targets (wasm
excluded from v1 by design, not counted here).

| # | Category | Blast radius (facet-cc) | Smallest repro | Blocks |
|---|---|---|---|---|
| 1 | Workspace-dependency shorthand (`dep = { workspace = true }`) treated as a per-name allowlist instead of a generic key check | 1356 grep hits workspace-wide; `dependency_is_workspace` only recognizes `blake3`/`autocfg` | `facet/Cargo.toml:136` `static_assertions = { workspace = true, optional = true }` → `detailed_dependency_of` unwraps `None` on `.get("version")` | A |
| 2 | Package-level `field.workspace = true` inheritance (`edition.workspace = true`, `rust-version.workspace = true`, `license.workspace = true`, `repository.workspace = true`) has only one hardcoded path (`package_edition` always reads `workspace_package_doc`, ignoring an explicit non-inherited `edition` if present) | 147 files use `.workspace = true` at the package-field level | any member whose `package.edition` is NOT inherited (none currently in facet-cc, but cargo's own root manifest sets `rust-version = "1.96"` directly while `edition.workspace = true` — mixed inheritance per-field is real) | A |
| 3 | No profile-derived rustc flags at all (not opt-level, not debug-assertions, not overflow-checks, not panic strategy, not codegen-units) — confirmed by exhaustive grep of `vix/src` | every unit (880 in facet-cc, 528 in cargo) | any `rustc!` call site in `crate.vix` — none emit `-C opt-level=0` etc. | A/B |
| 4 | `[profile.dev.package.*]` / named custom profiles (`[profile.fuzz]`, `[profile.profiling]`) unmodeled | 14 per-package overrides + 3 named profiles on this branch's root manifest | `Cargo.toml:520` `[profile.dev.package.backtrace] opt-level = 3` | A/B |
| 5 | Feature resolution (optional deps, `dep:` syntax, weak `pkg?/feat`, feature unification across normal/build/dev scopes) — distilled to prose (`rodin/docs/60-features.md`) but not implemented natively; the one fixture testing it is `#[ignore]`d | 175 `optional = true` deps, 61 `[features]` sections workspace-wide | `vix/tests/rodin_fixtures.rs:936` `#[ignore = "pending optional weak feature activation fix in rodin.vix"]` | A |
| 6 | cfg/target gating (`rodin/docs/70-targets-cfg.md`) implemented only for the single-edge fixture shape; no test exercises >1 cfg-gated dependency in the same resolve | 21 real `[target.'cfg(...)']` tables, 299/2909 cfg-gated resolve edges | `vox/rust/vox-rt/Cargo.toml:23,27` (both `cfg(not(wasm32))` and `cfg(wasm32)` deps in the same manifest) | A |
| 7 | Generic build walk (`crate.vix` `ResolvedGraph` walker) is arity-dispatched (0/1/2/dynamic deps) and never exercised past 2 direct deps or ~4 total packages | fixture ceiling: `lock_graph` = 4 packages, deepest chain `mini_app → alpha_lib → core_lib` | `vix/tests/fixtures` has no fixture over ~8 packages | A/B |
| 8 | No member-glob expansion (`members = ["crates/*"]`) | 0 in facet-cc (all 146 members are explicit paths) — but real on cargo (`crates/*`, `credential/*` → 25 of cargo's 27 members) | `cargo-oracle/Cargo.toml:2-6` | A (blocks cargo as a workspace target; does not block facet-cc today) |
| 9 | No mixed workspace+package root manifest (`[workspace]` and `[package]` in the same `Cargo.toml`) | 0 in facet-cc (pure virtual root) — real on cargo (`cargo = { path = "" }`, root package name="cargo") | `cargo-oracle/Cargo.toml:1` + `:147` | A (blocks cargo; does not block facet-cc) |
| 10 | Build-script protocol subset is narrower than facet-cc's real build.rs usage — implemented env vars are the slice-3a list (`OUT_DIR`, `CARGO_MANIFEST_DIR`, `CARGO_PKG_*`, `TARGET`/`HOST`/`OPT_LEVEL`/`PROFILE`); no `cargo:rustc-link-lib`/`cargo:rustc-link-search` consumption shown wired into a real multi-crate parent, only single-fixture `build_directives()` parsing | 12 real build.rs files; only `facet-core`/`facet`'s (trivial, `rustc-cfg` only) are in the tested fixture | any build.rs emitting `cargo:rustc-link-lib=` (none in facet-cc today — but `snark-scanner-host/build.rs` uses `cc::Build`, uncharacterized here since it's outside the vix fixture set) | B |
| 11 | Native/system-probing build scripts (Tier B "hermeticity boss fight") | facet-cc: 1 latent case (`aws-lc-sys`, currently unreachable in resolve); cargo: 6 live cases (`curl-sys`, `libgit2-sys`, `libssh2-sys`, `libz-sys`, `libnghttp2-sys`, `libsqlite3-sys`) all spawning `pkg-config`/`curl-config`/`git` | `curl-sys/build.rs` `Command::new("pkg-config")` | B (facet-cc: dormant risk; cargo: blocking) |
| 12 | `[patch.crates-io]` (85 entries, facet-cc only) unmodeled — no evidence any `.vix` code reads `[patch]` at all | 85 patch entries, all path-redirecting workspace crates back onto themselves | `Cargo.toml:523-608` | A |
| 13 | Non-`lib`/`bin` crate-types (`cdylib`+`rlib`, `cdylib`+`staticlib`) unmodeled — `crate.vix`'s target-shape functions hardcode `crate_type: "lib"` or `"bin"` | 10 crates in facet-cc (styx-wasm, styx-ffi, snark-wasm, vox-inprocess, subject-rust, 2× wasm-*-tests) | `styx-ffi/Cargo.toml:16` `crate-type = ["cdylib", "staticlib"]` | A/C (wasm explicitly out of v1 scope per RESURRECTION, but cdylib/staticlib on native targets isn't) |
| 14 | No array-of-struct return / no string→Path constructor — self-documented language gaps | n/a (language-level, blocks any manifest surface needing them) | `cargo_manifest.vix:255-271` `target_shapes_array_gap()`, `resolved_unit_adaptation_gap()` | A/B (infra) |

## 7. Rodin resolve wall-clock

No wall-clock number exists for rodin resolving anything close to
monorepo scale — there is no fixture that large. What exists:

- `cargo nextest run -p vix --features real-process` runs the 5-fixture
  differential rodin corpus (`vix/tests/rodin_fixtures.rs`) inside the
  168-test suite; individual test times for the rodin-touching tests are
  9-26s each (dominated by process/VM startup and full suite fixed costs
  under nextest, not solve time specifically — the harness doesn't isolate
  a pure-solve timer).
- `RESURRECTION.md`'s only concrete solve-adjacent number is the CDCL trail
  micro-benchmark (a synthetic 2048-push ladder unrelated to a real
  manifest resolve): 1.6-3.7ms → 70.8µs after the consuming-move fold. That
  is a data-structure micro-benchmark, not a "resolve N packages" number,
  and should not be extrapolated to monorepo scale.
- Given §3/§6 (features and cfg gating not yet natively implemented, no
  fixture over ~8 packages), a monorepo-scale rodin resolve cannot run
  today — there's no code path that would produce a correct answer for it,
  let alone a timed one. This is itself the most direct answer to "note
  wall-clock at whatever scale you reach": the reachable scale is ~8
  packages, single-digit seconds dominated by process startup, and it
  cannot currently be pushed further without hitting gaps #1, #5, or #6
  above.

## 8. Burndown list (blast-radius order, from §6)

1. Generic workspace-dependency-shorthand resolution (`workspace = true`
   read as a key, not a name allowlist) — unblocks the largest single
   surface (1356 hits) and is a prerequisite for almost every other gap to
   even be reachable at scale.
2. Profile-derived rustc flags (dev profile at minimum; per-package
   overrides and named profiles after) — currently silently absent, not
   partially present, so any artifact-comparison test today is comparing
   against an implicit "no profile flags" cargo invocation, which happens
   to produce *a* valid binary but not the one cargo would produce.
3. Feature resolution (rodin `60-features.md` → native) — un-ignore the
   existing fixture first, then scale the fixture corpus past single-edge
   cases.
4. cfg/target gating (rodin `70-targets-cfg.md` → native) past the
   single-edge fixture.
5. Generalize the build walk past arity 0/1/2 (mostly already generalized
   via the `_dynamic_deps` path; needs a fixture that actually exercises
   3+ direct deps to prove it, plus the array-of-struct/Path-from-String
   language gaps it's pinned against).
6. `[patch.crates-io]` and non-lib/bin crate-types (cdylib/staticlib) —
   lower blast radius (85 and 10 respectively) but both are "not modeled
   at all," not "modeled with edge-case bugs."
7. Member-glob expansion and mixed workspace+package manifests — zero
   blast radius against facet-cc itself; required only if cargo's own repo
   (or any external crate with this shape) becomes an ingestion target.
8. Tier B native-toolchain closures (`pkg-config`/`curl-config`/`git`
   spawns inside `-sys` build scripts) — not urgent for facet-cc (one
   latent, unreachable case) but is the correct thing to design against
   *before* claiming Tier B generally, since `aws-lc-sys` going live is one
   feature-flag change away, and it's the wall the doc's own Slice-1 write-up
   already anticipates ("Header contents discovered by a real compiler
   through the host filesystem are not observed unless the command grammar
   declares them as inputs").

## Answer to the question

**Not yet, at any tier.** Tier A (resolve+plan matching cargo's unit graph)
is blocked by #1-#6 above, all with measured, non-hypothetical blast radius
against the real 146-member/863-package workspace. Tier B adds profile
flags and the build-script protocol gap. Tier C (full link) wasn't probed
further since it's gated behind A and B. The honest scale claim today is:
"vixen's generic build walk matches cargo's unit graph for hand-picked
sub-graphs of ≤8 packages with no features, no cfg gating, and no
workspace-dependency shorthand" — which is real and load-bearing progress
(the demand-driven walk genuinely generalizes past the single-package Slice
1 goal), but it is fixture-scale, not workspace-scale, and the two are
separated by a specific, countable list rather than a vague "needs more
work."
