# 05 — The fixture corpus (acceptance behaviors)

The oracle (doc 00) needs fixtures. rodin carried a corpus of tiny offline
path-dependency Cargo workspaces, each isolating exactly one resolver behavior and
checked against `cargo tree -e normal,build --target <triple>`. These are the
acceptance behaviors the native implementation must reproduce; each is stated as a
workspace shape plus the invariant it pins. They also enumerate the corner cases
that motivate features (60) and cfg gating (70) — the "what must be right" list.

Two targets recur: `x86_64-unknown-linux-gnu` (LINUX) and
`x86_64-pc-windows-msvc` (WINDOWS).

## 1. Optional dep must NOT over-activate (the jiff-static shape)

`app → lib`. `lib` has an optional dep `helper`, referenced only by:
- a non-default feature `static-tz = ["dep:helper"]`,
- a weak feature `tz-fat = ["helper?/tz-fat"]` (with `default = ["tz-fat"]`),
- and a second, mandatory `helper` edge gated `cfg(any())` (always false — the
  version-locking hack).

Invariant: `helper` is selected on **no** target. `dep:` is not enabled (its
feature is off), weak `?/` never activates a missing dep, and `cfg(any())` never
fires. This is the over-activation bug the corpus exists to catch.

## 2. Default feature → target-conditional optional dep

`app → lib`. `lib` has `default = ["bundle-platform"]`, `bundle-platform =
["dep:platform"]`, and `platform` is optional and declared only for
`cfg(windows)`.

Invariant: `platform` selected on WINDOWS, not on LINUX. The feature is on (by
default), but the optional-dep edge's cfg gate still filters by target.

## 3. Direct target-conditional edge (the winapi shape)

`app → winthing`, edge gated `cfg(windows)`.

Invariant: `winthing` present on WINDOWS, absent on LINUX.

## 4. Build-dependency is consumed on every target

`app --build-dep--> gen`.

Invariant: `gen` selected on every target (build deps are part of the host graph;
Oracle 2's `-e normal,build` includes them).

## 5. Transitive dev-dependency is NOT consumed

`app → lib`, and `lib --dev-dep--> testonly`.

Invariant: `testonly` not selected by a normal build of `app`. Dev-deps are
consumed only for the crate being tested (the root), not transitively.

## The harness is retargeted, not rebuilt

`rodin-fixtures` already is this: a fixture DSL that emits a real path-dependency
Cargo workspace (offline, no registry), runs `cargo tree -e normal,build --target`
as the oracle, runs the system under test, and asserts equal per-target selection.
It is kept and **retargeted** — the only Rust-coupled part is the SUT
(`rodin_core::CompiledIndex`), which is swapped to rodin.vix reading the same
emitted workspace. The DSL, the workspace emission, and the cargo-oracle call
stay; the few pieces it needs from the deleted crates (the cfg-expression type,
the emission helpers) move into the harness.

## This corpus must grow — a lot

Five fixtures barely scratch it. The corpus is a living asset; grow it as the
build advances, at least to: compat-class coexistence (`serde@1` + `serde@2`),
version backtracking and conflict learning, registry (non-path) resolution,
prerelease admission (doc 30's open gap), feature unification across diamond
deps, and weak-dep-feature corners. This doc is the catalog of covered-vs-missing;
keep it current as fixtures land.
