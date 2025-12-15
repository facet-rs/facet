# Benchmark Tooling Scripts

This directory contains tools for running and analyzing benchmarks.

## Files

### `bench-report.sh`
Main entry point for generating benchmark reports.

**Usage:**
```bash
./bench-report.sh          # Generate report
./bench-report.sh --serve  # Generate and auto-serve on http://localhost:1999
```

**What it does:**
1. Sets up Python venv with uv (first run only)
2. Runs divan benchmarks (wall-clock)
   - **Live progress indicator**: Shows line count updating every 0.5s
3. Runs gungraun benchmarks (instruction counts)
   - **Live progress indicator**: Shows line count updating every 0.5s
4. Calls Python parser to generate HTML report
5. Creates `report.html` symlink to latest
6. **If `--serve`**: Starts HTTP server on port 1999
7. Falls back to simple text embedding if Python fails

**Output:** `../bench-reports/report-TIMESTAMP.html` + `report.html` symlink

**Progress indicator:**
Shows real-time feedback while benchmarks run:
```
📊 Running divan (wall-clock) ... 342 lines
```

This proves the benchmark isn't frozen and gives a sense of progress.

### `parse_bench.py`
Python script that parses benchmark output and generates HTML with tables and graphs.

**Usage:**
```bash
# Via venv (recommended)
.venv/bin/python parse_bench.py divan.txt gungraun.txt output.html

# Or directly
python3 parse_bench.py divan.txt gungraun.txt output.html
```

**What it does:**
- Parses divan output using regex (benchmark names, times, units)
- Parses gungraun output using regex (instruction counts, cache metrics)
- Calculates speedups (vs fastest, vs serde_json)
- Generates HTML with:
  * Styled, sortable tables
  * Chart.js bar charts
  * Color coding (fastest=green, JIT=yellow, serde=baseline)
  * Git metadata

**Dependencies:** None (uses only Python stdlib)
- `re` - regex parsing
- `json` - JSON generation for Chart.js
- `subprocess` - git commands
- `pathlib`, `datetime` - utilities

### `pyproject.toml`
Python project metadata for uv.

- Declares this as an installable Python package
- No external dependencies
- Entry point: `parse-bench` command

### `.venv/` (gitignored)
Python virtual environment created by uv on first run.

**Recreate if needed:**
```bash
rm -rf .venv
uv venv
uv pip install -e .
```

## Development

### Testing the Parser

```bash
# Get some benchmark data
cd ../facet-json
cargo bench --bench unified_benchmarks_divan --features cranelift --features jit > /tmp/divan.txt 2>&1
cargo bench --bench unified_benchmarks_gungraun --features cranelift --features jit > /tmp/gungraun.txt 2>&1

# Test the parser
cd ../scripts
python3 parse_bench.py /tmp/divan.txt /tmp/gungraun.txt /tmp/test-report.html

# View result
open /tmp/test-report.html
```

### Adding New Metrics

To add new metrics from gungraun (e.g., L2 cache misses):

1. Update `parse_gungraun_output()` regex if needed
2. Add display in `generate_html_report()` gungraun section

### Adding New Chart Types

The current implementation has bar charts. To add line charts, scatter plots, etc.:

1. Add new chart canvas in HTML template
2. Add new Chart.js initialization in the `<script>` section
3. Format data appropriately for the chart type

## Troubleshooting

### `uv: command not found`

Install uv:
```bash
curl -LsSf https://astral.sh/uv/install.sh | sh
```

### Parser fails with NameError

Make sure all variables used in f-strings are defined before use. Common issue: referencing variables from JavaScript section in Python code.

### No benchmarks parsed

Check the regex patterns in `parse_bench.py`:
- Divan: looks for `├─ benchmark_name` and `│  ├─ target_name  123.45 µs`
- Gungraun: looks for `unified_benchmarks_gungraun::module::benchmark_name` (legacy `gungraun_jit::...` still supported) and `  Instructions: 12345`

If divan/gungraun output format changes, update the regex.

### Charts not rendering

- Check browser console for JavaScript errors
- Verify Chart.js CDN is accessible
- Check JSON formatting in Chart.js data sections

## See Also

- **Main docs:** `../docs/BENCHMARKING.md` - Complete benchmarking guide
- **Debugging:** `../.claude/skills/debug-with-valgrind.md` - How to debug crashes
