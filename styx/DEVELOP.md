# Developing Styx

## Running Tests

```bash
# Run all tests
cargo nextest run

# Run tests for a specific crate
cargo nextest run -p styx-format
```

## Property Testing

The formatter uses [proptest](https://proptest-rs.github.io/proptest/) to find edge cases through fuzz testing. Property tests are in `crates/styx-format/src/cst_format.rs`.

Two invariants are tested:

1. **Semantics preservation** - formatting must not change the document's meaning (tree equality ignoring spans)
2. **Idempotence** - `format(format(x)) == format(x)`

Run with more cases to find rare bugs:

```bash
PROPTEST_CASES=5000 cargo nextest run -p styx-format proptests
```

When proptest finds a failing case, it saves it to `proptest-regressions/` for deterministic replay.

## Installing the CLI

```bash
cargo xtask install
```

This builds a release binary, copies it to `~/.cargo/bin/styx`, and codesigns it on macOS.
