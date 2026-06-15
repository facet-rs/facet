# Dependency audit

Audit note from the June 2026 dependency-tree pass. No dependency changes were
made as part of this audit; this is attribution and candidate ordering only.

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

The first `--target all` pass needed `moire-wasm`, which was not cached. After
allowing Cargo network access, Cargo downloaded `moire-wasm v2.0.0-rc.0` into
the registry cache. No files in the workspace were edited by that download.

## Snapshot

The host/default normal dependency graph resolved to roughly 289 unique
package-version entries during the audit.

The dependency mass is concentrated in a few places:

- Moire is a direct dependency of seven workspace packages.
- `facet-reflect` is reached by many core paths and enables `regex` by default.
- `facet-value` reaches into `facet-format`, which currently brings solver and
  formatting ergonomics along with value handling.
- `rust-examples` bundles several unrelated benchmark/demo tools into one crate.
- `xtask` pulls the Deno/SWC/dprint stack through in-process TypeScript
  formatting.

## Moire

Moire is the strongest first slimming candidate, but not a trivial "replace with
Tokio" change.

Direct workspace package users:

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

A plausible Vox-owned replacement would be a small runtime-primitives crate or
module with native and wasm backends. It would expose only the primitives Vox
actually uses, keep optional names for observability, and avoid pulling any
runtime graph/debugger machinery. This is semantically different from simply
changing every call site to raw Tokio, because raw Tokio does not solve the wasm
facade problem.

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
on `facet-format`, and `facet-format/default` brings:

- `facet-toml`
- `facet-solver`
- `facet-solver` suggestions
- `strsim`
- `figue`

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

### `facet-error`

`vox-core` declares `facet-error`, but a source grep under `rust/vox-core/src`
did not find `facet_error` usage.

Candidate:

- Remove the direct `vox-core` dependency and verify with `cargo check`.

Note: `figue` is another dependent of `facet-error`; `facet-error` itself is
not what pulls `figue` into `vox-core`.

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

`rust-examples` is a major aggregation point. It has one Cargo package for
several unrelated tools, so compiling any example package context pays for all
of these roots:

- `hdrhistogram` via `examples/bench_client.rs`
- `sysinfo` via `examples/bench_runner.rs`
- `indicatif` via `examples/shootout.rs`
- `serde` and `serde_json` via `examples/shootout.rs`

Candidate:

- Split heavyweight benchmark/shootout tools into separate crates, or gate them
  behind package features.

This is especially attractive because these dependencies are not core Vox
runtime dependencies; they are observation/benchmark tooling.

## `xtask` and TypeScript formatting

`xtask` uses `dprint-plugin-typescript` in-process for generated TypeScript
formatting. That single dependency brings the Deno/SWC stack, including many
proc macros and duplicate dependency families.

Observed related mass includes:

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

Candidate:

- Use an external formatter command for generated TypeScript, or split
  TypeScript formatting into a separate helper crate/tool so normal Rust
  workspace checks do not pay for the full Deno/SWC stack.

This should be weighed against reproducibility of generated TypeScript output.

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

These are good low-risk cleanup candidates, subject to checking target-specific
or pending work before removal.

## Duplicate-version families

Notable duplicate-version families from `cargo tree --duplicates`:

- `getrandom`
  - `0.3.x` through `tungstenite`/`rand`
  - `0.4.x` through `tempfile`, `vox-core`, wasm-bindgen, and WASI target
    branches
- `hashbrown`
  - older versions through SWC/dprint
  - newer versions through `facet-reflect` and `indexmap`
- `object`
  - `0.37.x` through `backtrace` and build support
  - `0.39.x` through `copypatch` build dependencies
- `thiserror`
  - `1.x` through `terminal-light`
  - `2.x` through `tungstenite` and Deno/SWC
- `rand`
  - `0.8.x` through `phf_generator`
  - `0.9.x` through `tungstenite`
- `syn`
  - `1.x` through `dprint-core-macros`
  - `2.x` through many modern proc macro dependencies

Most of these are second-order effects of the Moire, dprint/SWC,
terminal-theme, and websocket branches. It is probably more useful to shrink
the root branches first than to chase the duplicates directly.

## Suggested order for a future thinning pass

1. Design the Vox-owned runtime primitive facade that can replace Moire on both
   native and wasm, or patch Moire to expose a truly lightweight facade without
   `tokio/full` and runtime graph dependencies.
2. Remove workspace dependency declarations that have no package edge.
3. Tighten `facet-reflect`, `facet-pretty`, and any unused `facet-error` edge.
4. Split or feature-gate heavyweight `rust-examples` tools.
5. Isolate `xtask` TypeScript formatting so the Deno/SWC stack is not paid by
   ordinary Rust workspace graph analysis.

