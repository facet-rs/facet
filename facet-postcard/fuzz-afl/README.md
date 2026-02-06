# facet-postcard AFL Harness

This harness fuzzes `facet_postcard::from_slice` against malformed inputs and
ensures decoding failures are handled as errors rather than panics.

## Prerequisites

```bash
cargo install cargo-afl
```

## Build

```bash
cd facet-postcard/fuzz-afl
cargo afl build --bin from_slice
```

## Fuzz

```bash
mkdir -p in out
cargo afl fuzz -i in -o out target/debug/from_slice
```

You can seed `in/` with known regressions (for example, the `issue_2027` input).
