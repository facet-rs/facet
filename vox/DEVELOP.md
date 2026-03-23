# Development Guide

## Prerequisites

- **Rust**: stable toolchain with `cargo nextest` and `cargo insta` installed
- **Node.js** (v25+): for TypeScript subjects — install via `nvm` or system package
- **pnpm**: for TypeScript package management
- **Swift** (macOS only): for Swift subjects and runtime
- **just**: task runner — `cargo install just` or `brew install just`

## Running the Full Test Suite

`cargo nextest run` runs **all** tests, including cross-language spec tests that
need external subjects built first. Running it without building subjects will
produce failures like `"failed to spawn subject: No such file or directory"`.

To run everything from a clean state:

```bash
# 1. Build Rust subject (needed by spec tests)
cargo build --package subject-rust

# 2. Install TypeScript dependencies (needed by TS spec tests)
cd typescript && pnpm install && cd ..

# 3. Build Rust FFI staticlib (needed by Swift)
cargo build --release -p vox-shm-ffi

# 4. Build Swift runtime in debug mode (needed by SHM cross-language tests —
#    the shm-guest-client binary is only looked up under .build/debug/)
swift build --package-path swift/vox-runtime

# 5. Build Swift subject (needed by Swift spec tests)
swift build -c release --package-path swift/subject

# 6. Run all tests
cargo nextest run
```

Or use the `just` recipes which handle build dependencies automatically:

```bash
just interop-all   # builds + tests all languages (Rust, TypeScript, Swift)
```

### Running Tests by Language

```bash
# Rust spec tests (builds subject-rust automatically)
just rust

# TypeScript spec tests (typechecks, runs codegen, builds TS subject)
just ts

# Swift spec tests (builds FFI lib, runs swift tests, builds subject)
just swift

# All of the above
just interop-all
```

### Running Unit Tests Only

If you only want Rust unit/integration tests (no cross-language spec tests):

```bash
cargo nextest run --workspace --exclude spec-tests
```

### TypeScript Development

```bash
# Type-check TypeScript
cd typescript && pnpm check

# Or from repo root
just ts-typecheck

# Regenerate TypeScript bindings from spec-proto
cargo xtask codegen --typescript

# Or
just ts-codegen
```

### Swift Development

```bash
# Run spec tests with Swift subject (builds Rust FFI lib automatically)
just swift

# Build just the Rust FFI staticlib
just rust-ffi
# or: cargo build --release -p vox-shm-ffi

# Run root Swift package tests the same way CI does
cargo build --release -p vox-shm-ffi
swift test --no-parallel -Xlinker -L$(pwd)/target/release

# Build the Swift subject package
swift build -c release --package-path swift/subject

# If you specifically want the vox-runtime package path form, build the Rust
# staticlib first. The root-level `swift test ... -L$(pwd)/target/release`
# command is the preferred validation path in this repo.
swift test --package-path swift/vox-runtime --no-parallel -Xlinker -L$(pwd)/target/release

# Run SHM cross-language tests (Rust host, Swift guest)
cargo nextest run -p vox-shm --test bootstrap_cross_language
```

#### Consuming VoxRuntime from another Swift package

VoxRuntime depends on `libvox_shm_ffi.a`, a Rust staticlib. Consumers must:

1. Build the staticlib: `cargo build --release -p vox-shm-ffi` (from the vox workspace root)
2. Tell the linker where to find it:
   - **SPM CLI**: `swift build -Xlinker -L<path-to-vox>/target/release`
   - **SPM test**: `swift test -Xlinker -L<path-to-vox>/target/release`
   - **Xcode**: Add `<path-to-vox>/target/release` to `LIBRARY_SEARCH_PATHS`

### Code Generation

```bash
# Generate all language bindings
cargo xtask codegen

# Generate specific language
cargo xtask codegen --typescript
cargo xtask codegen --swift
cargo xtask codegen --go
cargo xtask codegen --java
cargo xtask codegen --python
```

### CI Checks

```bash
# Run all CI checks locally
cargo xtask ci

# Individual checks
cargo xtask test
cargo xtask clippy
cargo xtask fmt
cargo xtask doc
```

### Fuzzing

```bash
# List available fuzz targets
just fuzz-targets

# Build all fuzz targets
just fuzz-build all

# Build one target
just fuzz-build protocol_decode

# Run all targets for 60s each (build + run)
just fuzz all 60

# Run one target for 5 minutes (build + run)
just fuzz testbed_mem_session 300

# Run only (no build), useful for repeated local sessions
just fuzz-run protocol_decode 300

# Fuzz with ASAN
just fuzz-asan shm_link_roundtrip 300

# Fuzz with UBSAN
just fuzz-ubsan protocol_decode 300

# Fuzz with AFL++ SAND mode (native + sanitizer workers)
just fuzz-sand protocol_decode 300
```

Current targets:
- `framing_peek` (SHM frame parser)
- `shm_link_roundtrip` (SHM send/recv roundtrip)
- `protocol_decode` (Vox wire message decode/encode)
- `testbed_mem_session` (generated `spec-proto` RPC traffic over in-memory session/driver)

SAND recipes build three binaries per target under `fuzz/.sand/<target>/`:
- `native`
- `asan` (built with `AFL_USE_ASAN=1 AFL_LLVM_ONLY_FSRV=1`)
- `ubsan` (built with `AFL_USE_UBSAN=1 AFL_LLVM_ONLY_FSRV=1`)

## Project Structure

- `rust/` - Rust implementation (vox, vox-session, vox-codegen, etc.)
- `swift/` - Swift implementation
  - `vox-runtime/` - VoxRuntime Swift package (SHM transport, RPC, codegen)
  - `subject/` - Test subject for compliance suite
- `typescript/` - TypeScript implementation
  - `packages/vox-core/` - Core runtime
  - `packages/vox-tcp/` - TCP transport
  - `packages/vox-ws/` - WebSocket transport
  - `generated/` - Generated bindings (don't edit manually)
  - `subject/` - Test subject for compliance suite
- `spec/` - Protocol specification
  - `spec-proto/` - Service definitions for testing
  - `spec-tests/` - Compliance test suite
- `xtask/` - Development task runner

## Test Architecture

The compliance suite (`spec-tests`) tests protocol implementations by:
1. Starting a "subject" (server implementation in any language)
2. Acting as a client to verify protocol compliance
3. Testing both server-mode and client-mode scenarios

Subject selection is via `SUBJECT_CMD` environment variable.
