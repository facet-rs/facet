# facet development guide

## Website

The project's website is [facet.rs](https://facet.rs). The website source files live in `docs/`.

The site is deployed automatically (GitHub Pages) on pushes to `main` via `.github/workflows/website.yml`.

To run it locally, install [dodeca](https://github.com/bearcove/dodeca) and run `ddc serve` from the repo root.

## Local commands

The `Justfile` is the source of truth for local/CI commands. Start with:

- `just` (lists tasks)
- `just test <nextest args>` (wraps `cargo nextest run <filters>`)
- `just clippy` / `just clippy-all`
- `just docs` / `just doc-tests`
- `just nostd` (catches accidental `std` usage in core crates)

## Git hooks (Captain)

This repo uses [Captain](https://github.com/bearcove/captain) for pre-commit/pre-push automation.

- Hook scripts live in `hooks/` and invoke `captain` / `captain pre-push`.
- Configuration lives in `.config/captain/config.yaml`.
- Install hooks for the main repo and all git worktrees with `hooks/install.sh` (also wired via `conductor.json`).

If you bypass hooks with `--no-verify`, CI will still enforce the checks.

## Testing and debugging

Do yourself a favor and run tests with [cargo-nextest](https://nexte.st). `cargo test` is not the supported default workflow in this repo.

Project-wide nextest configuration (including debug profiles) lives in `.config/nextest.toml`.

Common workflows:

- **Valgrind**: use nextest profiles instead of hand-rolling valgrind invocations:
  - `cargo nextest run --profile valgrind <filters>`
  - `cargo nextest run --profile valgrind-lean <filters>`
  - `just valgrind <nextest args>` (wraps the profile)
- **LLDB on macOS**: `cargo nextest run --profile lldb <filters>`
- **Miri**: `just miri` runs a curated subset of the workspace under Miri (strict provenance). Use this when changing unsafe boundaries or anything that “should be impossible”.

## Generated files

Many `README.md` files are generated from `README.md.in`. Edit the `.in` file and let Captain regenerate (via hooks), or run `captain` manually if you don’t have hooks installed.

## Architecture pointers

- `facet-core`: defines the `Facet` trait and shape model — an unsafe boundary.
- `facet-reflect`: safe reflection/build/peek/poke built on top of `Shape`.
- `facet-format`: shared format core used by `facet-json`, `facet-yaml`, `facet-toml`, etc. (large blast radius).
- JIT deserialization lives in `facet-format` behind the `jit` feature; see `.claude/skills/jit-overview/SKILL.md` for orientation.

## Metrics

Compile-time and binary-size tracking exists, but is only relevant for perf/CI investigations. See `METRICS.md`.

## Rust nightly / MSRV

facet does not use Rust nightly, on purpose. It is "the best of stable". However,
the MSRV will likely bump with every new Rust stable version for the foreseeable
future.
