# Metrics (compile time, size, perf investigations)

Most changes don’t need any metrics work. This exists for performance investigations and CI/regression tracking.

## Compile-time and size measurements

Run an experiment:

```bash
cargo xtask measure <experiment-name>
```

Outputs:

- Human-readable report: `reports/YYYY-MM-DD-HHMM-<sha>-<name>.txt`
- Machine-readable log: `reports/metrics.jsonl`

View the history/comparisons:

```bash
cargo xtask metrics
```

## Prerequisites

The measurement pipeline uses nightly-only rustc flags.

- Install a nightly toolchain (as needed)
- `cargo-llvm-lines` is required for some measurements (`cargo install cargo-llvm-lines`)
- `summarize` from `measureme` may be required for some self-profile summaries (`cargo install --git https://github.com/rust-lang/measureme summarize`)
