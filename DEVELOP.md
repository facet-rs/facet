# facet development guide

## Website

The project's website is [facet.rs](https://facet.rs). The website source files live in `docs/`.

The site is deployed automatically (GitHub Pages) on pushes to `main` via `.github/workflows/website.yml`.

To run it locally, install [dodeca](https://github.com/bearcove/dodeca) and run `ddc serve` from the repo root.

## Local commands

The `Justfile` is the source of truth for local/CI commands. Start with:

- `just` (lists tasks)
- `just test <nextest args>` (wraps `cargo nextest run <filters>`)
- `just clippy` / `just clippy-all` (default workspace members)
- `just clippy-workspace` / `just clippy-workspace-all` (all workspace members)
- `just docs` / `just doc-tests`
- `just nostd` (catches accidental `std` usage in core crates)

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

## Running the tests on Windows (QEMU)

The Windows CI lane (`.github/workflows/test-platforms.yml`) can be reproduced
locally against a cached Windows 11 VM, without a Windows machine. The harness
lives in `scripts/winvm/` and is driven from the `Justfile`:

- `just win-vm-up` — first run downloads the Windows 11 Enterprise Evaluation
  ISO, performs a fully unattended install (`scripts/winvm/autounattend.xml.template`),
  enables OpenSSH, and installs `cargo-nextest.exe` + Node on the guest. Takes
  ~20-40 min the first time; afterwards it just boots the cached image and
  **leaves it running**.
- `just test-windows [nextest args]` — cross-compiles a nextest archive for
  `x86_64-pc-windows-msvc` on this host (via `cargo-xwin`), ships it plus the
  workspace source to the VM over SSH, and runs the suite there with the same
  exclusions as CI. Extra args are forwarded to `nextest run`.
- `just win-vm-ssh` — interactive shell on the VM.
- `just win-vm-down` — stop the VM (image preserved).
- `just win-vm-clean` — delete the cached image/ISO/keys to rebuild from scratch.

All toolchain and VM dependencies (the `windows-msvc` Rust target, `cargo-xwin`,
`qemu`, `swtpm`, `OVMF`, `xorriso`, …) are provided by the flake dev shell, so
the recipes run themselves through `nix develop`. Cached VM state lives under
`~/.cache/facet-winvm` (override with `FACET_WINVM_CACHE`); KVM (`/dev/kvm`) is
required. If the eval ISO fwlink rotates, set `FACET_WINVM_ISO_URL` or drop your
own ISO at `~/.cache/facet-winvm/windows.iso`.

## Generated files

Some `README.md` files are generated from crate rustdoc using `cargo reedme`.
If a README includes `cargo-reedme` markers, edit `src/lib.rs` docs and regenerate with:

- `cargo +nightly reedme --package <crate>`

## Architecture pointers

- `facet-core`: defines the `Facet` trait and shape model — an unsafe boundary.
- `facet-reflect`: safe reflection/build/peek/poke built on top of `Shape`.
- `facet-format`: shared format core used by `facet-json`, `facet-yaml`, `facet-toml`, etc. (large blast radius).
- `weavy`: shared bytecode-plan pieces extracted from Phon for format-owned VMs.

## Metrics

Compile-time and binary-size tracking exists, but is only relevant for perf/CI investigations. See `METRICS.md`.

## Rust nightly / MSRV

facet does not use Rust nightly, on purpose. It is "the best of stable". However,
the MSRV will likely bump with every new Rust stable version for the foreseeable
future.
