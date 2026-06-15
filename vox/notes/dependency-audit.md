# Dependency audit

Audit note from the June 2026 dependency-tree pass. No dependency changes were
made as part of the original audit; this note now also records the follow-up
thinning passes.

## Follow-up status

The first audit item, replacing Moire with a small Vox-owned runtime facade,
landed in PR #379. The workspace now has `vox-rt`/`vox-rt-macros`, and
`cargo tree --workspace -i moire -e normal -e features --format "{p} {f}"`
reports that no `moire` package is present.

The next low-risk cleanup pass removed stale `[workspace.dependencies]`
declarations, removed `vox-core`'s unused direct `facet-error` dependency, and
changed the shared `facet-pretty` dependency to `default-features = false`.

A follow-up examples pass removed the benchmark harness from `rust-examples`.
The benchmark clients/report generators should live in a separate repository
that consumes Vox from the outside, instead of living in the main workspace as
example targets.

The next pass removed `xtask`'s in-process TypeScript formatter. TypeScript
codegen now writes the generator output directly instead of routing it through
`dprint-plugin-typescript`.

The next pass removed proc-macro crate-name detection from
`vox-macros-core`. `#[vox::service]` generated code now assumes the runtime
crate is available as `vox`, which lets the macro core drop `facet-cargo-toml`
and the TOML/Facet format stack it pulled in just to parse `Cargo.toml`.

Checks from that pass:

- `cargo metadata --format-version 1 --no-deps` plus a mechanical comparison
  between package dependency names and `[workspace.dependencies]` keys now shows
  no unused shared dependency declarations.
- `cargo tree -p vox-core -i facet-error -e normal -e features --format "{p} {f}"`
  reports that `facet-error` is absent from the `vox-core` package graph.
- `cargo tree -p vox -i terminal-light -e normal -e features --format "{p} {f}"`
  reports that `terminal-light` is absent from the standalone `vox` package
  graph.
- Workspace-wide `terminal-light` is still present because `xtask` uses `figue`,
  whose default feature stack activates `facet-pretty/default`. That is an
  `xtask`/tooling graph issue, not a direct `vox` runtime dependency.
- Workspace-wide `facet-error` is still present through `figue`; it is no
  longer a direct `vox-core` edge, and `facet-cargo-toml` is no longer present
  in the workspace graph.
- `rust-examples` no longer contains `bench_client`, `bench_runner`, `shootout`,
  or their report assets, and no longer directly depends on `hdrhistogram`,
  `indicatif`, `serde`, `serde_json`, `sysinfo`, `subject-rust`, `vox-ffi`,
  `facet`, or `spec-proto`.
- `cargo tree --workspace -i dprint-plugin-typescript -e normal -e features --format "{p} {f}"`
  reports that `dprint-plugin-typescript` is absent.
- `cargo tree --workspace -i serde -e normal -e features --format "{p} {f}"`
  reports that `serde` is absent from the host/default Rust workspace graph.

## Scope and commands

Primary commands used:

- `cargo tree --workspace -i moire -e normal -e features --format "{p} {f}"`
- `cargo tree --workspace --duplicates`
- `cargo tree --workspace --target all --duplicates`
- targeted reverse trees for `regex`, `tokio`, `facet-format`,
  `facet-pretty`, `serde`, `serde_json`, `dprint-plugin-typescript`,
  `sysinfo`, `hdrhistogram`, and several duplicate-version families
- `cargo metadata --format-version 1 --no-deps` to compare
  `[workspace.dependencies]` declarations with actual workspace package edges
- `tracey_status` to confirm the project Tracey setup before treating this as a
  repo-local note rather than a spec change
- follow-up reverse-tree checks for `moire`, `facet-error`, `facet-pretty`, and
  `terminal-light`
- `cargo fmt --all --check`
- `cargo check --workspace --all-targets --message-format=short`
- `cargo check -p xtask --message-format=short`
- `cargo xtask codegen --typescript`
- `cd typescript && pnpm check`
- `tracey_validate`

The first `--target all` pass needed `moire-wasm`, which was not cached. After
allowing Cargo network access, Cargo downloaded `moire-wasm v2.0.0-rc.0` into
the registry cache. No files in the workspace were edited by that download.

## Snapshot

The host/default normal dependency graph resolved to roughly 289 unique
package-version entries during the original audit. After the Moire, examples,
stale-declaration, and TypeScript-formatter removals, the broad workspace graph
resolves to roughly 264 unique package-version entries in the same
`cargo tree --workspace --prefix none --format "{p}"` count.

After PR #379, Moire is no longer in the dependency graph. The remaining
dependency mass is concentrated in a few places:

- `facet-reflect` is reached by many core paths and enables `regex` by default.
- `facet-value` reaches into `facet-format`, which currently brings solver and
  formatting ergonomics along with value handling.
- `rust-examples` bundles several unrelated benchmark/demo tools into one crate.
- `xtask` still pulls the `figue` CLI stack, including rich diagnostics and
  terminal-theme support through current default features.

## Moire

Status: completed by PR #379. The historical notes below explain why that was
the right first target and what shape the replacement needed.

Historical direct workspace package users:

- `vox`
- `vox-core`
- `vox-types`
- `vox-stream`
- `vox-phon`
- `subject-rust`
- `spec-tests`

Observed Vox usage is mostly a runtime primitive facade:

- `moire::task::spawn`
- `moire::spawn`
- `moire::task::FutureExt::named`
- `moire::sync::mpsc`
- `moire::sync::oneshot`
- `moire::sync::{Notify, Semaphore, SyncMutex, SyncRwLock}`
- `moire::time::{sleep, timeout}`
- a small number of `#[moire::instrument]` annotations

The async-debugger/dashboard side does not appear to be the reason Vox uses
Moire right now. The practical value is the cross-target facade and named
primitive API.

Important cost: even with Moire diagnostics off, `moire-tokio` and
`moire-runtime` depend on Tokio with `features = ["full"]` on native. That
widens the whole workspace Tokio feature set to include things like `fs`,
`process`, `signal`, `rt-multi-thread`, `parking_lot`, and related support
crates.

Moire also pulls the runtime graph stack:

- `moire-runtime`
- `moire-types`
- `moire-wire`
- `moire-trace-types`
- `moire-trace-capture`
- `moire-macros-noop`
- `ctor`
- `facet-json` / `facet-value` through the Moire runtime path
- `moire-wasm` for all-target/wasm analysis

The removal problem is therefore two-part:

1. Replace native Tokio-like primitives.
2. Preserve the WebAssembly-compatible abstraction Moire currently provides.

The landed replacement is the Vox-owned `vox-rt` runtime-primitives crate plus
`vox-rt-macros`. It exposes only the primitives Vox actually uses, keeps named
task/instrumentation surfaces where Vox needs them, and avoids pulling Moire's
runtime graph/debugger machinery.

## Facet tree

### `facet-reflect` and `regex`

`regex` is present because `facet-reflect/default = ["std", "regex"]`.

The `regex` code path is partial type-plan validation for `validate::regex`
attributes. Without the `regex` feature, `facet-reflect` falls back to literal
substring matching for that helper.

Candidate:

- If Vox does not need `validate::regex`, use `facet-reflect` with
  `default-features = false` and explicit `features = ["std"]` or whatever
  subset is actually needed.

Risk:

- This is a shared workspace dependency choice, so every direct
  `facet-reflect.workspace = true` user shares the resulting feature set.

### `facet-value` and `facet-format`

`facet-value` is not just a small dynamic value type in this graph. It depends
on `facet-format`, which brings the generic format/deserializer layer and:

- `facet-solver`
- `facet-solver` suggestions
- `strsim`

This looks primarily like an upstream feature-shaping problem. Vox can still
audit where it truly needs `facet-value`, but the cleaner fix is likely making
the `facet-value` to `facet-format` edge narrower upstream.

Current direct workspace users of `facet-value`:

- `spec-proto`
- `vox-types`
- `subject-rust`
- `vox-codegen` dev dependency
- `spec-tests`

### `facet-pretty`

`facet-pretty/default` enables `detect-terminal-theme`, which pulls
`terminal-light`, `crossterm`, and related terminal support.

Vox's observed direct use is `PrettyPrinter` in `rust/vox/src/server_logging.rs`
with `ColorMode::Never`.

Candidate:

- Try `facet-pretty = { default-features = false }` if server/client logging
  does not need terminal theme detection.

Status:

- Landed for the shared workspace dependency. Standalone `vox` no longer pulls
  `terminal-light`; workspace-wide analysis still sees it because `xtask` pulls
  `figue/default`.

### `facet-error`

`vox-core` declares `facet-error`, but a source grep under `rust/vox-core/src`
did not find `facet_error` usage.

Candidate:

- Remove the direct `vox-core` dependency and verify with `cargo check`.

Status:

- Landed for `vox-core`. A later macro-core cleanup also removed
  `facet-cargo-toml`, so the remaining workspace `facet-error` edge is through
  `figue`.

### Workspace `facet` feature coupling

The workspace-level `facet` dependency currently enables `camino` and `reflect`
globally, with default features also active. Many crates use `facet` only for
`Facet` derives or `Facet::SHAPE`.

Candidate:

- Split Facet dependency feature needs per crate instead of routing all Facet
  users through the broad workspace feature set.

Risk:

- Feature unification means one broad user can still widen the graph for the
  whole workspace unless the broad use is isolated.

## Examples

Status: the benchmark harness has been removed from this workspace. Future
benchmark work should happen in a separate repository that depends on Vox from
the outside.

Before removal, `rust-examples` was a major aggregation point. It had one Cargo
package for several unrelated tools, so compiling any example package context
paid for all of these roots:

- `hdrhistogram` via `examples/bench_client.rs`
- `sysinfo` via `examples/bench_runner.rs`
- `indicatif` via `examples/shootout.rs`
- `serde` and `serde_json` via `examples/shootout.rs`

That removal also dropped benchmark-only ties to `subject-rust`, `vox-ffi`,
`facet`, and `spec-proto` from `rust-examples`.

## `xtask` and TypeScript formatting

Status: removed from the workspace.

Before removal, `xtask` used `dprint-plugin-typescript` in-process for generated
TypeScript formatting. That single dependency brought the Deno/SWC stack,
including many proc macros and duplicate dependency families:

- `deno_ast`
- `swc_common`
- `swc_ecma_ast`
- `swc_ecma_lexer`
- `swc_ecma_parser`
- `dprint-core`
- `dprint-swc-ext`
- `syn@1`
- additional `syn@2` users
- extra `hashbrown` versions
- `serde`
- `url`/ICU-related dependencies through the SWC/Deno branch

The landed cleanup removes the formatter dependency entirely. `cargo xtask
codegen --typescript` now writes raw generator output and the checked-in
generated TypeScript was regenerated from that path. `cd typescript && pnpm
check` still passes.

## Declared workspace dependencies with no package edge

`cargo metadata --no-deps` showed these entries in `[workspace.dependencies]`
with no workspace package dependency edge at the time of the audit:

- `arc-swap`
- `cbindgen`
- `divan`
- `facet-format`
- `facet-postcard`
- `hyper-util`
- `loom`
- `moire-types`
- `museair`
- `prost`
- `static_assertions`
- `tarpc`
- `tokio-stream`
- `tonic`
- `tonic-prost`
- `tonic-prost-build`
- `tower`
- `ulid`
- `ur-taking-me-with-you`

Status:

- Removed the stale declarations. A fresh mechanical comparison between
  `[workspace.dependencies]` keys and package dependency names now reports no
  unused shared dependency declarations.

## Duplicate-version families

Notable duplicate-version families from `cargo tree --duplicates`:

- `getrandom`
  - `0.3.x` through `tungstenite`/`rand`
  - `0.4.x` through `tempfile`, `vox-core`, wasm-bindgen, and WASI target
    branches
- `hashbrown`
  - the SWC/dprint branch that introduced older versions has been removed
- `object`
  - `0.37.x` through `backtrace` and build support
  - `0.39.x` through `copypatch` build dependencies
- `thiserror`
  - `1.x` through `terminal-light`
  - `2.x` through `tungstenite`
- `syn`
  - the old `syn@1` branch from `dprint-core-macros` has been removed
  - `syn@2` remains through modern proc macro dependencies and `xtask`'s
    Rust-source generation path

Most remaining duplicate families are second-order effects of the facet/phon,
terminal-theme, and websocket branches. It is probably more useful to shrink
the root branches first than to chase the duplicates directly.

## Suggested order for a future thinning pass

1. Tighten `facet-reflect` if Vox does not need `validate::regex` behavior from
   `facet-reflect/default`.
2. Investigate whether `figue` can be used by `xtask` without rich diagnostics,
   or whether CLI parsing should move away from the default `figue` stack.
