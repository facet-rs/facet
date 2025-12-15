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
# Generate report
./scripts/bench-report.sh

# Generate report and auto-serve on http://localhost:1999
./scripts/bench-report.sh --serve
```

**What it does:**
1. Set up Python virtual environment with uv (first run only)
2. Run all divan benchmarks (wall-clock times)
   - **Live progress**: Shows line count updating in real-time
3. Run all gungraun benchmarks (instruction counts)
   - **Live progress**: Shows line count updating in real-time
4. Parse the output with `parse_bench.py` (tables, graphs, speedup calculations)
5. Generate HTML report with Chart.js visualizations
6. Save to `bench-reports/report-TIMESTAMP.html`
7. Create `report.html` symlink to latest
8. **If `--serve`**: Automatically start HTTP server on port 1999

**Progress indicator:**
While benchmarks run, you'll see live updates:
```
📊 Running divan (wall-clock) ... 342 lines
🔬 Running gungraun (instruction counts) ... 89 lines
```

This shows the benchmarks are still running (not frozen).

**First run setup:**
- Creates `scripts/.venv` using uv
- Installs the parse_bench module
- Subsequent runs reuse the venv

**View the report:**
```bash
# Auto-serve (easiest!)
./scripts/bench-report.sh --serve
# Opens http://localhost:1999/report.html

# Or manually
open bench-reports/report.html

# Or specific timestamped report
open bench-reports/report-20251215-152959.html
```

## Benchmark Types

### Divan Benchmarks (Wall-Clock Time)

Located in `facet-json/benches/unified_benchmarks_divan.rs` (auto-generated via `cargo xtask gen-benchmarks`).

**Run all benchmarks:**
```bash
cd facet-json
cargo bench --bench unified_benchmarks_divan --features cranelift --features jit
```

**Run specific benchmark:**
```bash
cargo bench --bench unified_benchmarks_divan simple_struct --features cranelift --features jit
cargo bench --bench unified_benchmarks_divan twitter --features cranelift --features jit
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
2. `facet_json_cranelift` - JSON-specific JIT (requires `--features cranelift` or alias `--features jit`)
3. `facet_format_json` - Format-agnostic interpreter
4. `facet_format_jit` - Format-agnostic JIT ⭐ (enabled by `--features jit`)
5. `serde_json` - Industry baseline 🎯

### Gungraun Benchmarks (Instruction Counts)

Located in `facet-json/benches/unified_benchmarks_gungraun.rs` (auto-generated).

**Run benchmarks:**
```bash
cd facet-json
cargo bench --bench unified_benchmarks_gungraun --features cranelift --features jit
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
cargo bench --bench unified_benchmarks_divan --features cranelift --features jit > divan-output.txt 2>&1

# Gungraun
cargo bench --bench unified_benchmarks_gungraun --features cranelift --features jit > gungraun-output.txt 2>&1
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
    - Run: cargo bench --bench unified_benchmarks_gungraun --features cranelift --features jit
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

Both Divan and Gungraun benches are generated from `facet-json/benches/benchmarks.kdl`. To add a benchmark:

1. **Define the type (if needed).**
   ```kdl
   type_def name="MyType" {
       code """
#[derive(Debug, PartialEq, Facet, serde::Serialize, serde::Deserialize, Clone)]
struct MyType {
    value: u64,
}
"""
   }
   ```
2. **Add a benchmark entry** that either embeds JSON or points at a file in `benches/data/`.
   ```kdl
   benchmark name="my_type" type="MyType" category="micro" {
       json "{\"value\": 42}"
   }
   ```
3. **Regenerate the benches.**
   ```bash
   cargo xtask gen-benchmarks
   ```
   This rewrites `unified_benchmarks_divan.rs` and `unified_benchmarks_gungraun.rs`.
4. **Re-run the benches** (divan + gungraun) and update the report.

The generator automatically adds:
- Divan benches for the 5 targets (facet_format_jit, facet_format_json, facet_json, facet_json_cranelift, serde_json)
- Gungraun benches (including cached JIT warmups and cranelift groups)

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
- `facet-json/benches/benchmarks.kdl` - Source of truth for generator inputs
- `facet-json/benches/unified_benchmarks_divan.rs` - Divan benchmarks (auto-generated)
- `facet-json/benches/unified_benchmarks_gungraun.rs` - Gungraun benchmarks (auto-generated)
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

## Report Features

The generated HTML reports include:

### Visual Design
- **Emojis**: Each target has an emoji for quick identification
  * ⚡ facet_format_jit (Format JIT) - Gold/Yellow
  * 🚀 facet_json_cranelift (JSON JIT) - Light Teal
  * 📦 facet_format_json (Format Interp) - Red
  * 🔧 facet_json (JSON Interp) - Teal
  * 🎯 serde_json (Baseline) - Purple

- **Color Coding** (consistent across ALL tables and charts):
  * Fastest: Green background
  * facet_format_jit: Yellow background (our star!)
  * serde_json: Purple background (the baseline)

### Interactive Features
- **Hover table row** → highlights corresponding bar in chart
- **Visual feedback** - dimmed bars, thicker border on highlighted
- **Smooth transitions** - no laggy animations

### Data Organization
- **Separate serialize/deserialize** - each operation gets own table+chart
- **Categorized benchmarks**:
  * 🔬 Micro (simple_struct, nested, options)
  * 🌍 Realistic (twitter, canada, hashmaps)
  * 📊 Arrays (Vec<T> benchmarks)
- **One table + one chart per benchmark** - easy to scan

### Speedup Calculations
- **vs Fastest** - absolute performance (1.0x = fastest)
- **vs serde_json** - our goal (how close are we?)
- **Color coded:**
  * Green: ≤1.0x (beating or matching baseline!)
  * Orange: 1.0-1.5x (close)
  * Red: >1.5x (work to do)

### Chart Details
- **Horizontal bars** - easier to read labels
- **Sorted by speed** - fastest at top
- **Consistent colors** - same target always same color
- **Tooltips** - hover bar for exact time
- **No legend clutter** - labels are on bars
