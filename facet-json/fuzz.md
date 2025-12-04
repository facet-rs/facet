# Fuzzing facet-json

This crate uses [Bolero](https://camshaft.github.io/bolero/) for fuzzing the JSON scanner and token adapter.

**Note:** All fuzzing backends (libfuzzer, AFL, honggfuzz) require **nightly Rust**.

## Quick Start

```bash
# Install nightly
rustup toolchain install nightly

# Install cargo-bolero
cargo install cargo-bolero

# Fuzz the scanner (runs until you Ctrl+C)
cargo +nightly bolero test -p facet-json --features bolero-inline-tests \
    scanner::fuzz_tests::fuzz_scanner_arbitrary_bytes

# Fuzz the adapter
cargo +nightly bolero test -p facet-json --features bolero-inline-tests \
    adapter::fuzz_tests::fuzz_adapter_arbitrary_bytes
```

Let it run for minutes to hours. The longer it runs, the more edge cases it finds.

## Quick Sanity Check (Stable Rust)

The inline tests also work on stable, but only run random inputs for ~1 second each (not real fuzzing):

```bash
cargo test -p facet-json --lib --features bolero-inline-tests
```

Good for CI to catch obvious panics, but won't find subtle bugs.

### Available Fuzz Targets

**Scanner targets** (in `src/scanner.rs`):
- `fuzz_scanner_arbitrary_bytes` - Raw bytes, should never panic
- `fuzz_scanner_json_like` - Bytes wrapped in `[...]`
- `fuzz_decode_string` - String decoding
- `fuzz_scanner_strings` - Quoted strings with arbitrary content
- `fuzz_scanner_numbers` - Number-like inputs
- `fuzz_parse_number` - Direct number parsing
- `fuzz_scanner_nested` - Deeply nested structures

**Adapter targets** (in `src/adapter.rs`):
- `fuzz_adapter_arbitrary_bytes` - Raw bytes through adapter
- `fuzz_adapter_skip` - Skip functionality
- `fuzz_adapter_next_skip_alternating` - Alternating next/skip
- `fuzz_adapter_no_borrow` - BORROW=false path
- `fuzz_adapter_string_escapes` - Strings with escape sequences
- `fuzz_adapter_skip_nested` - Skip on nested structures

## What We're Testing

1. **No panics** - The scanner/adapter should handle any input without panicking
2. **No infinite loops** - Should always terminate
3. **Memory safety** - No buffer overflows or out-of-bounds access
4. **Correctness** - When `BORROW=false`, strings are always `Cow::Owned`

## Debugging Failures

When Bolero finds a crash, it saves the input to a corpus. To reproduce:

```bash
# Show the failing input
cargo bolero reduce -p facet-json scanner::fuzz_tests::fuzz_scanner_arbitrary_bytes

# Run with a specific input file
cargo bolero test -p facet-json scanner::fuzz_tests::fuzz_scanner_arbitrary_bytes --corpus path/to/input
```

## Fuzzing Engines

Bolero supports multiple fuzzing engines:

```bash
# Use libfuzzer (default, requires nightly)
cargo +nightly bolero test -p facet-json scanner::fuzz_tests::fuzz_scanner_arbitrary_bytes

# Use AFL
cargo bolero test -p facet-json scanner::fuzz_tests::fuzz_scanner_arbitrary_bytes --engine afl

# Use honggfuzz
cargo bolero test -p facet-json scanner::fuzz_tests::fuzz_scanner_arbitrary_bytes --engine honggfuzz
```

## CI Integration

The inline tests run automatically with `cargo test --features bolero-inline-tests`. They use a fixed iteration count for reproducibility.

For deeper fuzzing in CI, consider running for a fixed time:

```bash
timeout 300 cargo bolero test -p facet-json scanner::fuzz_tests::fuzz_scanner_arbitrary_bytes || true
```
