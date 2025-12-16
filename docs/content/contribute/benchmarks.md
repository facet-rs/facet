+++
title = "Benchmark System"
weight = 45
+++

facet includes a comprehensive benchmark system for measuring runtime performance of
JSON deserialization across multiple implementations. This page covers how to define,
run, and analyze benchmarks.

## Quick start

```bash
# Generate benchmark code from definitions
cargo xtask gen-benchmarks

# Run all benchmarks and generate HTML report
cargo xtask bench

# Run with live server
cargo xtask bench --serve

# Skip running benchmarks, reuse previous data
cargo xtask bench --no-run
```

## Architecture overview

The benchmark system consists of three main tools:

| Tool | Purpose |
|------|---------|
| `benchmark-defs` | Shared types for parsing benchmark definitions (KDL) |
| `benchmark-generator` | Generates Rust benchmark code from KDL definitions |
| `benchmark-analyzer` | Runs benchmarks, parses output, generates HTML reports |

```
facet-json/benches/benchmarks.kdl     (source of truth)
        │
        ▼
    benchmark-generator
        │
        ├──► unified_benchmarks_divan.rs    (wall-clock timing)
        └──► unified_benchmarks_gungraun.rs (instruction counts)
                    │
                    ▼
            benchmark-analyzer
                    │
                    └──► bench-reports/report.html
```

## Benchmark definition format

All benchmarks are defined in `facet-json/benches/benchmarks.kdl`. This is the **single
source of truth** — the generator reads this file and produces benchmark code for all
targets automatically.

### Basic structure

```kdl
// Define a benchmark
benchmark name="simple_struct" type="SimpleRecord" category="micro" {
    json "{\"id\": 42, \"name\": \"test\", \"active\": true}"
}

// Define the type used by the benchmark
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

### Benchmark properties

| Property | Description |
|----------|-------------|
| `name` | Unique identifier for the benchmark (used in module names) |
| `type` | Rust type to deserialize into (must have a matching `type_def`) |
| `category` | One of: `micro`, `synthetic`, `realistic` |

### Data sources

Each benchmark must have exactly one data source:

```kdl
// Inline JSON string
benchmark name="foo" type="Foo" category="micro" {
    json "{\"field\": \"value\"}"
}

// Generated data (for arrays, maps, etc.)
benchmark name="integers" type="Vec<u64>" category="synthetic" {
    generated "integers"
}

// Brotli-compressed corpus file
benchmark name="twitter" type="TwitterResponseSparse" category="realistic" {
    json_brotli "corpus/twitter.json.br"
}
```

### Available generators

For `generated` data sources, these generators are available:

| Generator | Output |
|-----------|--------|
| `booleans` | 10,000 alternating bools |
| `integers` | 1,000 large u64 values |
| `floats` | 1,000 f64 values |
| `short_strings` | 1,000 strings (~10 chars each) |
| `long_strings` | 100 strings (1000 chars each) |
| `escaped_strings` | 1,000 strings with `\n`, `\t`, `\"`, `\\` |
| `hashmaps` | 1,000 key-value pairs |
| `nested_structs` | 500 nested struct instances |
| `options` | 500 structs with optional fields |

### Categories

Benchmarks are grouped by category in the generated report:

| Category | Description |
|----------|-------------|
| `micro` | Tiny inputs testing minimal overhead |
| `synthetic` | Generated data testing specific patterns (arrays, strings, etc.) |
| `realistic` | Real-world JSON files (twitter, canada geojson, citm_catalog) |

## Benchmark targets

Every benchmark runs against 5 targets:

| Target | Description |
|--------|-------------|
| `facet_format_jit` | Format-agnostic JIT deserializer (the main work!) |
| `facet_json_cranelift` | JSON-specific JIT using Cranelift |
| `facet_format_json` | Format-agnostic interpreter |
| `facet_json` | JSON-specific interpreter |
| `serde_json` | Baseline comparison |

> **Note:** JIT targets (`facet_format_jit`) are skipped for `realistic` benchmarks
> because the JIT compiler doesn't yet support complex nested types.

## Benchmark harnesses

Two harness types are generated:

### Divan (wall-clock timing)

[Divan](https://github.com/nvzqz/divan) measures actual elapsed time. Results show:
- Fastest/slowest/median/mean times
- Sample count and iteration count

```bash
cargo bench --bench unified_benchmarks_divan --features cranelift --features jit
```

### Gungraun (instruction counts)

[Gungraun](https://github.com/iai-callgrind/iai-callgrind) (via iai-callgrind) measures
CPU instructions executed. This is deterministic and machine-independent.

```bash
cargo bench --bench unified_benchmarks_gungraun --features cranelift --features jit
```

## HTML report

The analyzer parses benchmark output and generates an interactive HTML report with:

- **Summary charts**: Bar charts comparing facet-format+jit vs serde_json per category
- **Detailed tables**: All 5 targets with timing, instruction count, and relative speed
- **Sidebar navigation**: Jump to specific benchmarks with scroll-based highlighting
- **Error display**: Missing/failed targets shown with "error" status

### Report features

- Targets sorted by speed (fastest first)
- Speed comparison vs serde_json baseline (e.g., "2.5× slower")
- Color-coded rows: green=fastest, blue=JIT highlight, gray=baseline, red=error
- Charts use Observable Plot for clean visualization

## Adding a new benchmark

1. **Add the type definition** (if new type needed):

```kdl
type_def name="MyType" {
    code """
#[derive(Debug, PartialEq, Facet, serde::Serialize, serde::Deserialize, Clone)]
struct MyType {
    // fields...
}
"""
}
```

2. **Add the benchmark**:

```kdl
benchmark name="my_benchmark" type="MyType" category="micro" {
    json "{...}"
}
```

3. **Regenerate and test**:

```bash
cargo xtask gen-benchmarks
cargo build -p facet-json --benches --features cranelift --features jit
cargo xtask bench
```

## Corpus files

Large real-world JSON files are stored brotli-compressed in:

```
tools/benchmark-generator/corpus/
├── canada.json.br      (GeoJSON - number-heavy)
├── citm_catalog.json.br (event ticketing data)
└── twitter.json.br     (API response with nested objects)
```

These are decompressed at runtime using `LazyLock` — the decompression happens once
and is not included in benchmark timing.

## File locations

| Path | Purpose |
|------|---------|
| `facet-json/benches/benchmarks.kdl` | Benchmark definitions (edit this!) |
| `facet-json/benches/unified_benchmarks_divan.rs` | Generated divan benchmarks |
| `facet-json/benches/unified_benchmarks_gungraun.rs` | Generated gungraun benchmarks |
| `tools/benchmark-defs/` | Shared KDL parsing types |
| `tools/benchmark-generator/` | Code generator |
| `tools/benchmark-analyzer/` | Report generator |
| `bench-reports/` | Output directory for reports and raw data |

## Troubleshooting

### Benchmark crashes (SIGSEGV)

This usually means a JIT compilation issue. Check if the failing benchmark is in the
`realistic` category — if so, ensure JIT is being skipped (set `category="realistic"`).

### Missing rows in report

Check if the benchmark is defined in `benchmarks.kdl`. The report now shows "error"
for missing targets instead of omitting them.

### Outdated generated code

Always run `cargo xtask gen-benchmarks` after editing `benchmarks.kdl`.
