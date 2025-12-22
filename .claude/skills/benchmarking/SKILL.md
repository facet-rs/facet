---
name: benchmarking
description: Run and manage performance benchmarks with cargo xtask bench for facet-json, analyzing results with Markdown reports and comparing against serde_json baseline
---

# Benchmarking with cargo xtask bench

The facet project uses a sophisticated benchmarking system that generates Markdown reports comparing performance across multiple targets.

## Quick Reference - Running Specific Benchmarks

```bash
# Run specific benchmark by name
cargo bench --bench unified_benchmarks_divan -- flatten_2enums

# Run with Tier-2 diagnostics
FACET_TIER2_DIAG=1 cargo bench --bench unified_benchmarks_divan -- flatten_2enums 2>&1 | grep TIER_DIAG

# Check tier2 statistics (attempts/successes/fallbacks)
cargo bench --bench unified_benchmarks_divan -- flatten_2enums 2>&1 | grep TIER_STATS

# Run all benchmarks matching a pattern
cargo bench --bench unified_benchmarks_divan -- flatten

# Run Tier-2 JIT benchmarks only
cargo bench --bench unified_benchmarks_divan -- "tier2"

# List available benchmarks
cargo bench --bench unified_benchmarks_divan -- --list | grep -v "    " | head -20
```

**⚠️  IMPORTANT:** Benchmark `.rs` files are GENERATED from `facet-json/benches/benchmarks.kdl`.
**DO NOT** edit `unified_benchmarks_*.rs` directly - edit `benchmarks.kdl` instead.

## Quick Usage

```bash
# Run all benchmarks and generate HTML + Markdown report
cargo xtask bench --index --serve

# Run benchmarks without the full perf index (faster)
cargo xtask bench

# Re-analyze existing benchmark data without re-running
cargo xtask bench --no-run

# Run only specific benchmarks (filter passed to cargo bench)
cargo xtask bench --index booleans

# Just generate reports from latest data
cargo xtask bench --no-run --index --serve
```

## How It Works

The benchmarking system has three main components:

### 1. Benchmark Definition (`benchmarks.kdl`)

Benchmarks are defined in `facet-json/benches/benchmarks.kdl` using KDL syntax:

```kdl
benchmark name="simple_struct" type="SimpleRecord" category="micro" {
    json "{\"id\": 42, \"name\": \"test\", \"active\": true}"
}

benchmark name="booleans" type="Vec<bool>" category="synthetic" {
    generated "booleans"
}

type_def name="SimpleRecord" {
    code """
#[derive(Debug, PartialEq, Facet, serde::Serialize, serde::Deserialize, Clone)]
struct SimpleRecord {
    id: u64,
    name: String,
    active: bool,
}
"""
}
```

**Categories**: `micro`, `synthetic`, `realistic`, `other`
**Data sources**: `json` (inline), `json_file`, `json_brotli`, `generated`

### 2. Benchmark Generation (`cargo xtask gen-benchmarks`)

Run this after editing `benchmarks.kdl`:

```bash
cargo xtask gen-benchmarks
```

This generates three files in `facet-json/`:
- `benches/unified_benchmarks_divan.rs` - Wall-clock timing benchmarks
- `benches/unified_benchmarks_gungraun.rs` - Instruction count benchmarks
- `tests/generated_benchmark_tests.rs` - Test versions for valgrind debugging

**Every benchmark gets all 4 targets automatically:**
- `serde_json` - Baseline (serde_json crate)
- `facet_format_json` - facet-format-json without JIT (reflection only)
- `facet_format_jit_t1` - Tier-1 JIT (shape-based, ParseEvent stream)
- `facet_format_jit_t2` - Tier-2 JIT (format-specific, direct byte parsing)

### 3. Benchmark Execution and Analysis

`cargo xtask bench` does:
1. Runs `unified_benchmarks_divan` (wall-clock times via divan)
2. Runs `unified_benchmarks_gungraun` (instruction counts via gungraun + valgrind)
3. Parses output and combines results
4. Generates multiple report formats:
   - `bench-reports/run.json` - Full structured data (schema: run-v1)
   - `bench-reports/perf/RESULTS.md` - **Markdown report for LLMs and humans**
   - `bench-reports/perf-data.json` - Legacy format for perf tracking

## The Markdown Report (`perf/RESULTS.md`)

Located at `bench-reports/perf/RESULTS.md`, this is the **authoritative source** for performance analysis:

**Structure:**
- **Targets table** - Definitions of all benchmark targets
- **Benchmark sections** - Grouped by category (Micro, Synthetic, Realistic)
- **Per-benchmark tables** - Deserialize and Serialize results
  - Columns: Target, Time (median), Instructions, vs serde_json ratio
  - Ratios: `**0.84×** ✓` (wins), `1.03×` (close), `3.12× ⚠` (needs work)
- **Summary** - Auto-categorized by performance:
  - Wins: ≤1.0× vs serde_json
  - Close: ≤1.5× vs serde_json
  - Needs Work: >1.5× vs serde_json

**Example:**
```markdown
### booleans

**Deserialize:**

| Target | Time (median) | Instructions | vs serde_json |
|--------|---------------|--------------|---------------|
| serde_json | 56.21µs | 1,157,922 | 1.00× |
| format+jit2 | 53.46µs | 972,221 | **0.84×** ✓ |
| format+jit1 | 809.30µs | 7,031,459 | 6.07× ⚠ |
| format | 2.94ms | 23,169,951 | 20.01× ⚠ |
```

## Adding New Benchmarks

1. **Edit `facet-json/benches/benchmarks.kdl`**
   ```kdl
   benchmark name="my_bench" type="MyType" category="synthetic" {
       generated "my_generator"
   }

   type_def name="MyType" {
       code """
   #[derive(Debug, Facet, serde::Serialize, serde::Deserialize, Clone)]
   struct MyType {
       field: String,
   }
   """
   }
   ```

2. **If using `generated`, add generator to `tools/benchmark-generator/src/main.rs`**
   - Edit `generate_json_data()` function
   - Add case for your generator name

3. **Regenerate benchmarks**
   ```bash
   cargo xtask gen-benchmarks
   ```

4. **Run benchmarks**
   ```bash
   cargo xtask bench --index --serve
   ```

## Important Flags

### `--no-run`
Skips running benchmarks, uses latest data. Useful for:
- Regenerating reports after fixing parser bugs
- Testing report generation changes
- Quick iterations on report formatting

### `--index`
Generates the full perf.facet.rs index:
- Clones the `facet-rs/perf.facet.rs` repo (gh-pages branch)
- Copies benchmark reports to `bench-reports/perf/`
- Generates index.html and supporting files
- **Required for viewing the interactive SPA**

### `--serve`
Starts a local server at `http://localhost:1999` to view reports.
Requires `--index`.

### `--push`
Pushes generated reports to the perf.facet.rs repo.
**Use with caution** - only for publishing official results.

## Debugging Benchmarks with Valgrind

The generated tests in `tests/generated_benchmark_tests.rs` mirror the benchmarks and can be run under valgrind:

```bash
# Run specific benchmark as test under valgrind
cargo nextest run --profile valgrind -p facet-json generated_benchmark_tests::test_booleans --features jit

# Or use the generated test filters
cargo nextest run --profile valgrind -p facet-json test_simple_struct --features jit
```

This is essential for debugging crashes or memory issues in benchmarks.

## Files and Directories

```
bench-reports/
├── divan-{timestamp}.txt          # Raw divan output
├── gungraun-{timestamp}.txt       # Raw gungraun output
├── run.json                       # Structured results (run-v1 schema)
├── perf-data.json                 # Legacy perf tracking format
└── perf/
    ├── RESULTS.md                 # **MAIN REPORT - READ THIS**
    ├── index.html                 # SPA (generated with --index)
    ├── app.js                     # SPA logic (copied from scripts/)
    └── shared-styles.css          # SPA styles (copied from scripts/)

facet-json/benches/
├── benchmarks.kdl                 # **EDIT THIS to add benchmarks**
├── unified_benchmarks_divan.rs    # Generated (divan)
└── unified_benchmarks_gungraun.rs # Generated (gungraun)

facet-json/tests/
└── generated_benchmark_tests.rs   # Generated (for valgrind)

tools/
├── benchmark-generator/           # KDL → Rust codegen
└── benchmark-analyzer/            # Output parsing + report generation
```

## Don't Edit Generated Files

❌ **NEVER edit these files** (they're regenerated):
- `unified_benchmarks_divan.rs`
- `unified_benchmarks_gungraun.rs`
- `generated_benchmark_tests.rs`
- `bench-reports/perf/index.html`, `app.js`, `shared-styles.css`

✅ **Edit these instead**:
- `facet-json/benches/benchmarks.kdl` - Benchmark definitions
- `tools/benchmark-generator/src/main.rs` - Generator logic (for `generated` benchmarks)
- `scripts/app.js`, `scripts/shared-styles.css` - SPA source (not the copies in perf/)

## Common Workflows

### Quick local benchmark run
```bash
cargo xtask bench
# Check bench-reports/perf/RESULTS.md
```

### Full interactive report
```bash
cargo xtask bench --index --serve
# Opens http://localhost:1999
```

### After editing benchmarks.kdl
```bash
cargo xtask gen-benchmarks
cargo xtask bench
```

### Re-analyze existing data
```bash
cargo xtask bench --no-run --index
```

### Benchmark a specific test
```bash
cargo xtask bench integers
# Only runs benchmarks matching "integers"
```

## Performance Analysis Tips

1. **Focus on the Markdown report first** (`perf/RESULTS.md`)
   - Easy to grep, parse, and read
   - Shows all critical metrics in one place
   - Auto-categorized by performance tier

2. **Use instruction counts, not just time**
   - More stable than wall-clock time
   - Architecture-independent
   - Appears in "vs serde_json" column when available

3. **Look for patterns in the Summary section**
   - "Needs Work" items are optimization targets
   - "Wins" validate current approach
   - "Close" items are low-hanging fruit

4. **Compare Tier-1 vs Tier-2 JIT**
   - Large gaps = Tier-2 not implemented or buggy
   - Similar performance = Tier-2 working but not optimized
   - Tier-2 wins = format-specific optimizations paying off

## Troubleshooting

### Benchmarks fail to compile
```bash
# Regenerate from KDL
cargo xtask gen-benchmarks
```

### Parser errors in output
- Check `bench-reports/divan-*.txt` or `gungraun-*.txt` for malformed output
- Fix the benchmark code, not the parser (usually)

### Missing benchmarks in report
- Ensure benchmark has `category` in `benchmarks.kdl`
- Check that `cargo xtask gen-benchmarks` ran successfully
- Verify benchmark functions are generated (check `unified_benchmarks_*.rs`)

### `--index` fails
- Ensure `gh` CLI is installed and authenticated
- Check network connection (clones from GitHub)
- Try `--index` without `--push` first

## See Also

- divan docs: https://docs.rs/divan/
- gungraun: Custom fork with valgrind integration
- Nextest valgrind profile: `.config/nextest.toml`
- Benchmark generator: `tools/benchmark-generator/`
- Report analyzer: `tools/benchmark-analyzer/`
