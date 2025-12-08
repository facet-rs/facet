+++
title = "Compile-Time Metrics"
weight = 50
+++

facet includes tooling to measure compile-time metrics and track them over time.
This helps benchmark changes that affect compilation speed or binary size.

## Quick start

```bash
# Take a measurement
cargo xtask measure my-experiment

# View metrics history
cargo xtask metrics
```

## How it works

The `measure` command runs three steps:

1. **Clean build** with `-Zmacro-stats`, `-Zprint-type-sizes`, and `--timings`
2. **LLVM IR analysis** via `cargo llvm-lines`
3. **Self-profile collection** via `-Zself-profile`

All measurements use `facet-bloatbench`, a crate with ~120 generated structs and
~40 enums that exercises the derive macro and vtable generation.

## Output files

| File | Description |
|------|-------------|
| `reports/YYYY-MM-DD-HHMM-<sha>-<name>.txt` | Human-readable report with full details |
| `reports/metrics.jsonl` | Machine-readable metrics (appended per run) |

## Metrics collected

### Compile time
- `compile_secs` — Total wall-clock compile time

### Binary size
- `bin_unstripped` — Binary size before `strip`
- `bin_stripped` — Binary size after `strip`

### LLVM IR
- `llvm_lines` — Total lines of LLVM IR
- `llvm_copies` — Number of monomorphized function copies

### Type sizes
- `type_sizes_total` — Sum of all facet-related type sizes (bytes)

### Self-profile (time in milliseconds)
- `typeck_ms` — Type checking
- `mir_borrowck_ms` — Borrow checking
- `expand_proc_macro_ms` — Proc macro expansion
- `eval_to_allocation_raw_ms` — Const evaluation
- `llvm_module_optimize_ms` — LLVM optimization passes
- `llvm_module_codegen_ms` — LLVM code generation
- `llvm_lto_optimize_ms` — LTO optimization
- `codegen_module_ms` — Rust codegen

## Viewing metrics

The interactive TUI lets you explore metrics history:

```bash
cargo xtask metrics
```

This reads `reports/metrics.jsonl` and displays trends across experiments.

## Typical workflow

```bash
# 1. Measure baseline on current code
cargo xtask measure baseline

# 2. Make your changes
# ...

# 3. Measure after changes
cargo xtask measure after-my-change

# 4. Compare in TUI
cargo xtask metrics
```

## Detailed self-profiling

For deep-dive analysis of const evaluation, trait resolution, etc., you can generate
a detailed profile with query arguments:

```bash
# Clean and build with detailed profiling
cargo clean -p facet-bloatbench
RUSTFLAGS="-Zself-profile=profile -Zself-profile-events=default,args" \
    cargo +nightly build -p facet-bloatbench --features facet

# Convert to Chrome trace format
crox profile/facet_bloatbench-*.mm_profdata

# Compress for analysis
gzip chrome_profiler.json
```

The `-Zself-profile-events=default,args` flag is key — without `args`, you only get
timing data but not the DefId details needed to identify specific const evaluations.

### Analyzing with Perfetto

[Perfetto](https://ui.perfetto.dev/) is a trace viewer that can load the Chrome trace
format and run SQL queries against the data. Open the `.json.gz` file directly.

Click "Query (SQL)" in the left sidebar to run queries. The key tables are:
- `slice` — timing spans (name, duration in nanoseconds)
- `args` — arguments attached to slices (contains DefId paths)

### Useful SQL queries

```sql
-- Total const evaluation time and count
SELECT
    COUNT(*) as count,
    SUM(dur)/1e9 as total_secs
FROM slice
WHERE name = 'eval_to_const_value_raw';

-- Top 20 most expensive const evaluations by path
SELECT
    SUBSTR(a.display_value, 1, 100) as const_path,
    COUNT(*) as count,
    SUM(s.dur)/1e6 as total_ms
FROM slice s
JOIN args a ON s.arg_set_id = a.arg_set_id
WHERE s.name = 'eval_to_const_value_raw'
GROUP BY const_path
ORDER BY total_ms DESC
LIMIT 20;

-- Count SHAPE-related const evals (facet's main overhead)
SELECT COUNT(*) as shape_evals
FROM slice s
JOIN args a ON s.arg_set_id = a.arg_set_id
WHERE s.name = 'eval_to_const_value_raw'
AND LOWER(a.display_value) LIKE '%shape%';

-- Analyze const eval by nesting depth
-- (nested &const {} blocks each trigger separate evals)
SELECT
    CASE
        WHEN a.display_value LIKE '%constant#0}::{constant#0}::{constant#%' THEN 'depth 3+'
        WHEN a.display_value LIKE '%constant#0}::{constant#%' THEN 'depth 2'
        WHEN a.display_value LIKE '%constant#%' THEN 'depth 1'
        ELSE 'other'
    END as nesting_depth,
    COUNT(*) as count,
    SUM(s.dur)/1e6 as total_ms
FROM slice s
JOIN args a ON s.arg_set_id = a.arg_set_id
WHERE s.name = 'eval_to_const_value_raw'
GROUP BY nesting_depth
ORDER BY total_ms DESC;

-- Breakdown of field vs vtable const evals in SHAPE
SELECT
    SUBSTR(a.display_value, INSTR(a.display_value, '::SHAPE'), 60) as pattern,
    COUNT(*) as count
FROM slice s
JOIN args a ON s.arg_set_id = a.arg_set_id
WHERE s.name = 'eval_to_const_value_raw'
AND a.display_value LIKE '%SHAPE%constant#0}::{constant#%'
GROUP BY pattern
ORDER BY count DESC
LIMIT 20;
```

### What to look for

- **`constant#0}::{constant#1}`** — field array inside SHAPE
- **`constant#0}::{constant#0}`** — vtable inside SHAPE
- **`constant#1}::{constant#N}`** — individual field N inside the array

High counts at depth 2+ indicate nested `&const {}` blocks that could be flattened.
Each nesting level adds const evaluation overhead.

## Prerequisites

These tools are required:

```bash
# Rust nightly (for -Z flags)
rustup install nightly

# cargo-llvm-lines
cargo install cargo-llvm-lines

# summarize (for self-profile aggregation)
cargo install --git https://github.com/rust-lang/measureme summarize

# crox (for Chrome trace conversion)
cargo install --git https://github.com/rust-lang/measureme crox
```

## Example output

```
=== Facet Compile-Time Measurement ===
Experiment: after-hrtb-removal
Report: reports/2025-12-09-0434-e44dd929-after-hrtb-removal.txt

Step 1/3: Clean build with macro-stats + type-sizes + timings...
  Compile time: 16.87s
  Binary size: 1588 KB (stripped: 1249 KB)
  Macro stats: 12 lines
  Type sizes: 528 facet lines, 17054 total, 34684 bytes total
Step 2/3: Running cargo llvm-lines...
    178603                4965                (TOTAL)
Step 3/3: Collecting rustc self-profile...
  Self-profile data collected

Summary:
  Compile time: 16.87s
  Binary size:  1588 KB (stripped: 1249 KB)
  LLVM lines:   178603 (4965 copies)
  Type sizes:   34684 bytes total
```
