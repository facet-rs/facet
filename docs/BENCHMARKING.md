# Benchmarking Guide

This document covers all benchmarking tools and workflows in the facet project.

## Overview

We use multiple benchmarking tools for different purposes:

1. **Divan** - Wall-clock time measurements (good for relative performance)
2. **Gungraun** - Deterministic instruction counts via Valgrind (reproducible, CI-friendly)
3. **Benchmark Report Generator** - Combines both into HTML reports with tables and graphs

## Prerequisites

### Required Tools

```bash
# Install gungraun-runner for deterministic benchmarks
cargo install gungraun-runner@0.17.0

# Install uv for Python environment management (report generator)
curl -LsSf https://astral.sh/uv/install.sh | sh
# Or: cargo install uv
```

**Note:** The benchmark report generator uses Python but has no external dependencies (just stdlib). However, we use `uv` for proper isolation.

## Quick Start

### Generate a Complete Benchmark Report

```bash
./scripts/bench-report.sh
```

This will:
1. Set up Python virtual environment with uv (first run only)
2. Run all divan benchmarks (wall-clock times)
3. Run all gungraun benchmarks (instruction counts)
4. Parse the output with `parse_bench.py` (tables, graphs, speedup calculations)
5. Generate HTML report with Chart.js visualizations
6. Save to `bench-reports/report-TIMESTAMP.html`

**First run setup:**
- Creates `scripts/.venv` using uv
- Installs the parse_bench module
- Subsequent runs reuse the venv

**View the report:**
```bash
# macOS/Linux with GUI
open bench-reports/report-TIMESTAMP.html

# Or via HTTP server
cd bench-reports
python3 -m http.server 8000
# Then visit http://localhost:8000/report-TIMESTAMP.html
```

## Benchmark Types

### Divan Benchmarks (Wall-Clock Time)

Located in `facet-json/benches/vs_format_json.rs`.

**Run all benchmarks:**
```bash
cd facet-json
cargo bench --bench vs_format_json --features cranelift
```

**Run specific benchmark:**
```bash
cargo bench --bench vs_format_json simple_struct --features cranelift
cargo bench --bench vs_format_json twitter --features cranelift
```

**Available benchmarks:**

**Micro Benchmarks** (for testing JIT features):
- `simple_struct` - Flat struct with primitives + String
- `single_nested_struct` - Nested struct (Outer { inner: Inner { ... } })
- `simple_with_options` - Struct with Option<i64>, Option<bool>, Option<f64>

**Realistic Benchmarks** (real-world data):
- `twitter` - Twitter API response (complex nested data)
- `canada` - GeoJSON with large coordinate arrays
- `hashmaps` - HashMap serialization/deserialization
- `nested_structs` - Vec<Outer> with 3-level deep nesting
- `floats`, `integers`, `booleans` - Homogeneous Vec<T> benchmarks
- `short_strings`, `long_strings` - Vec<String> benchmarks

**Targets compared:**
1. `facet_json` - Legacy interpreter
2. `facet_json_cranelift` - JSON-specific JIT (requires `--features cranelift`)
3. `facet_format_json` - Format-agnostic interpreter
4. `facet_format_jit` - Format-agnostic JIT ⭐
5. `serde_json` - Industry baseline 🎯

### Gungraun Benchmarks (Instruction Counts)

Located in `facet-json/benches/gungraun_jit.rs`.

**Run benchmarks:**
```bash
cd facet-json
cargo bench --bench gungraun_jit --features cranelift
```

**Prerequisites:**
```bash
cargo install gungraun-runner@0.17.0
```

**How it works:**
- Uses Valgrind's Callgrind to count instructions
- Deterministic - same code = same instruction count
- Perfect for CI regression detection
- Measures: Instructions, L1/LL cache hits, RAM hits, estimated cycles

**Available benchmarks:**
- `simple_struct` - All 5 targets with warmup for JIT caching
- `nested_struct` - All 5 targets with warmup
- More can be added to the benchmark group

**Key feature:** Warmup functions ensure we measure cached JIT execution, not compilation cost.

## Benchmark Report Generator

### Architecture

```
scripts/bench-report.sh  (Shell script - orchestrator)
    ↓ Creates uv venv (first run)
    ↓ Runs divan benchmarks → bench-reports/divan-TIMESTAMP.txt
    ↓ Runs gungraun benchmarks → bench-reports/gungraun-TIMESTAMP.txt
    ↓ Calls Python parser
scripts/parse_bench.py   (Python parser)
    ↓ Parses divan output (regex for times/units)
    ↓ Parses gungraun output (regex for instruction counts)
    ↓ Calculates speedups (vs fastest, vs serde_json)
    ↓ Generates HTML with:
      - Styled tables (color-coded by performance)
      - Chart.js bar charts (all 5 targets)
      - Git metadata (commit, branch, timestamp)
bench-reports/report-TIMESTAMP.html (Final output)
```

### Python Environment Setup

The report generator uses **uv** for Python environment management:

**Automatic setup** (via bench-report.sh):
- First run creates `scripts/.venv/` using `uv venv`
- Installs `scripts/` as editable package with `uv pip install -e .`
- Subsequent runs reuse the venv

**Manual setup:**
```bash
cd scripts
uv venv
uv pip install -e .
```

**Why uv?**
- Fast, reliable Python environment management
- Proper isolation without polluting global Python
- No external dependencies needed (script uses only stdlib)
- Deterministic builds

### Manual Usage

**Run benchmarks separately:**
```bash
cd facet-json

# Divan
cargo bench --bench vs_format_json --features cranelift > divan-output.txt 2>&1

# Gungraun
cargo bench --bench gungraun_jit --features cranelift > gungraun-output.txt 2>&1
```

**Generate report manually:**
```bash
python3 scripts/parse-bench.py divan-output.txt gungraun-output.txt report.html
```

### Report Features

The generated HTML report includes:

**Header:**
- Timestamp
- Git commit hash
- Git branch name

**Legend:**
- Explains all 5 targets being compared

**Wall-Clock Performance Section:**
- Tables for micro vs realistic benchmarks
- Each benchmark shows:
  * Target name
  * Median time (formatted: ns, µs, or ms)
  * Speedup vs fastest
  * Speedup vs serde_json 🎯
- Color coding:
  * Green = fastest
  * Yellow = our JIT
  * Gray = baseline (serde)

**Instruction Counts Section:**
- Shows gungraun deterministic measurements
- Key metrics: Instructions, Estimated Cycles
- Formatted with thousands separators

**Charts:**
- Bar charts using Chart.js
- Compare all 5 targets side-by-side
- Micro benchmarks chart
- Realistic benchmarks chart (future)

## CI Integration

### Gungraun in CI

Gungraun benchmarks run automatically on every PR via `.github/workflows/test.yml`:

```yaml
gungraun:
  runs-on: depot-ubuntu-24.04-32
  steps:
    - Install valgrind
    - Install gungraun-runner@0.17.0
    - Run: cargo bench --bench gungraun_jit --features cranelift
```

**Why in CI:**
- Deterministic results catch performance regressions
- Instruction counts are reproducible across runs
- Fast (single-run, not thousands of iterations)

**Viewing CI results:**
- Check the "gungraun" job in GitHub Actions
- Instruction count changes appear in job logs

## Interpreting Results

### Understanding Speedups

**vs Fastest** - How much slower than the absolute fastest target:
- `1.0x` = This IS the fastest
- `2.0x` = 2x slower than fastest
- Lower is better

**vs serde_json** - How we compare to the industry standard:
- `1.0x` = Matching serde_json! 🎉
- `<1.0x` = Faster than serde_json!! 🚀
- `>1.0x` = Slower than serde_json (our current state)

### JIT vs Interpreter Speedups

For format-agnostic deserialization, we care about:
- **facet_format_jit** vs **facet_format_json** (interpreter)
- Current results: **~2-2.2x speedup** on supported types

### Goals

1. **Short-term:** Beat interpreters by 2-3x (✅ ACHIEVED!)
2. **Medium-term:** Get within 2-5x of serde_json on structs
3. **Long-term:** Match serde_json on struct-heavy workloads

## Adding New Benchmarks

### Add to Divan (facet-json/benches/vs_format_json.rs)

```rust
mod my_new_benchmark {
    use super::*;

    #[derive(Facet, Serialize, Deserialize, Clone, Debug)]
    struct MyType {
        // ...
    }

    static DATA: LazyLock<MyType> = LazyLock::new(|| MyType { /* ... */ });
    static JSON: LazyLock<String> = LazyLock::new(|| facet_json::to_string(&*DATA));

    #[divan::bench]
    fn facet_format_jit_deserialize(bencher: Bencher) {
        bencher.bench(|| {
            black_box(format_jit::deserialize_with_fallback::<MyType, _>(
                facet_format_json::JsonParser::new(black_box(JSON.as_bytes())),
            ))
        });
    }

    // Add other targets: facet_format_json, facet_json, facet_json_cranelift, serde_json
}
```

### Add to Gungraun (facet-json/benches/gungraun_jit.rs)

```rust
fn setup_my_type() -> &'static [u8] {
    let json = br#"{"field": 42}"#;
    // Warmup to cache JIT
    let _ = format_jit::deserialize_with_fallback::<MyType, _>(JsonParser::new(json));
    json
}

#[library_benchmark]
#[bench::cached(setup = setup_my_type)]
fn my_type_facet_format_jit(json: &[u8]) -> MyType {
    let parser = JsonParser::new(black_box(json));
    black_box(format_jit::deserialize_with_fallback::<MyType, _>(parser).unwrap())
}

// Add to the benchmark group
library_benchmark_group!(
    name = jit_benchmarks;
    benchmarks = ..., my_type_facet_format_jit
);
```

**Important:** Always add warmup for JIT benchmarks to measure cached execution, not compilation!

### Update Parser for New Benchmarks

If adding a new benchmark module, update `scripts/parse-bench.py`:

```python
# Add to micro_benchmarks or realistic_benchmarks list
micro_benchmarks = ['simple_struct', 'single_nested_struct', 'simple_with_options', 'my_new_micro']
realistic_benchmarks = ['twitter', 'canada', 'hashmaps', 'my_new_realistic']
```

## Troubleshooting

### Gungraun not found

```bash
cargo install gungraun-runner@0.17.0
```

**Version must match:** gungraun lib version (0.17 in Cargo.toml) = gungraun-runner version

### Python parser fails

The shell script falls back to simple text embedding if Python fails. To debug:

```bash
python3 scripts/parse-bench.py divan.txt gungraun.txt test.html
```

### Charts not rendering

- Check browser console for errors
- Ensure Chart.js CDN is accessible
- Check HTML validation

### Benchmarks show unexpected results

1. **Check for compilation in measurement:**
   - JIT benchmarks should have warmup
   - Look for first-run compilation spikes

2. **Check CPU frequency scaling:**
   - Divan is sensitive to system load
   - Gungraun is immune (instruction counts)

3. **Compare gungraun vs divan:**
   - Should show similar relative orderings
   - If very different, investigate warmup or caching

## Best Practices

### DO:
- ✅ Use gungraun for regression detection in CI
- ✅ Use divan for development and relative comparisons
- ✅ Always add warmup to JIT benchmarks
- ✅ Compare all 5 targets (esp. vs serde_json)
- ✅ Generate reports for PR reviews
- ✅ Track realistic benchmarks (twitter, canada) not just micros

### DON'T:
- ❌ Trust single divan runs (noise!)
- ❌ Compare across different machines (use gungraun instead)
- ❌ Forget to warmup JIT (measures compilation not execution)
- ❌ Only look at micro benchmarks
- ❌ Ignore serde_json baseline

## Files Reference

### Benchmark Code
- `facet-json/benches/vs_format_json.rs` - Divan benchmarks (all targets)
- `facet-json/benches/gungraun_jit.rs` - Gungraun benchmarks (JIT focus)
- `facet-json/benches/vs_serde.rs` - Legacy serde comparisons

### Tooling
- `scripts/bench-report.sh` - Main entry point (runs benchmarks + generates report)
- `scripts/parse-bench.py` - Parser and HTML generator
- `.config/nextest.toml` - Nextest configuration (includes valgrind profile)
- `.github/workflows/test.yml` - CI job for gungraun

### Output
- `bench-reports/` - Generated reports and raw data (gitignored)
- `bench-reports/report-TIMESTAMP.html` - Final HTML report
- `bench-reports/divan-TIMESTAMP.txt` - Raw divan output
- `bench-reports/gungraun-TIMESTAMP.txt` - Raw gungraun output

## Related Documentation

- **Debugging:** See `.claude/skills/debug-with-valgrind.md` for debugging crashes
- **Divan docs:** https://github.com/nvzqz/divan
- **Gungraun docs:** https://gungraun.github.io/gungraun/
- **Nextest wrapper scripts:** https://nexte.st/docs/configuration/wrapper-scripts/
