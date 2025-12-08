# Fuzz Testing for Rapace

This directory contains fuzz targets for testing the robustness of rapace's parsing and validation code.

## Prerequisites

Fuzz testing requires nightly Rust (a `rust-toolchain.toml` file is provided in this directory).

Install cargo-fuzz:
```bash
cargo install cargo-fuzz
```

The fuzz targets will automatically use nightly Rust when run from this directory.

## Available Fuzz Targets

### `fuzz_header_decode`
Tests the message header decoding logic in `src/header.rs`. This fuzzer:
- Feeds arbitrary byte sequences to `MsgHeader::decode_from()`
- Ensures the decoder never panics, only returns `Err` for invalid input
- Tests bounds checking, encoding validation, and metadata parsing

### `fuzz_descriptor_validation`
Tests the descriptor validation logic in `src/frame.rs`. This fuzzer:
- Creates raw `MsgDescHot` descriptors from arbitrary bytes
- Validates them using `RawDescriptor::validate_inline_only()`
- Ensures validation never panics, only returns `Err` for invalid descriptors
- Tests inline payload bounds checking and payload length validation

## Running Fuzz Targets

Run a specific fuzz target:
```bash
cargo fuzz run fuzz_header_decode
```

Run with a time limit (e.g., 60 seconds):
```bash
cargo fuzz run fuzz_header_decode -- -max_total_time=60
```

Run with multiple jobs in parallel:
```bash
cargo fuzz run fuzz_header_decode -- -workers=8
```

List all available targets:
```bash
cargo fuzz list
```

## Checking Coverage

Generate coverage reports:
```bash
cargo fuzz coverage fuzz_header_decode
```

View the coverage report:
```bash
cargo cov -- show fuzz/target/*/release/fuzz_header_decode \
    --format=html \
    --instr-profile=fuzz/coverage/fuzz_header_decode/coverage.profdata \
    > fuzz/coverage/index.html
```

## Analyzing Crashes

If a fuzz target finds a crash, the failing input will be saved to:
```
fuzz/artifacts/fuzz_header_decode/crash-<hash>
```

Reproduce a crash:
```bash
cargo fuzz run fuzz_header_decode fuzz/artifacts/fuzz_header_decode/crash-<hash>
```

Minimize a crashing input:
```bash
cargo fuzz cmin fuzz_header_decode
```

## Continuous Integration

For CI, you can run each target for a fixed duration:
```bash
cargo fuzz run fuzz_header_decode -- -max_total_time=300
cargo fuzz run fuzz_descriptor_validation -- -max_total_time=300
```

## Adding New Fuzz Targets

1. Create a new file in `fuzz_targets/` (e.g., `fuzz_foo.rs`)
2. Add the target to `fuzz/Cargo.toml`:
   ```toml
   [[bin]]
   name = "fuzz_foo"
   path = "fuzz_targets/fuzz_foo.rs"
   test = false
   doc = false
   bench = false
   ```
3. Run it with `cargo fuzz run fuzz_foo`
