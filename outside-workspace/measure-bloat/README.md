# measure-bloat

A Rust utility for measuring and comparing binary sizes and build times between different serialization ecosystems.

## Overview

This tool compares three different scenarios:

1. **serde**: Using the serde ecosystem with ks-serde crates
2. **facet-pr**: Using facet from current PR/HEAD with ks-facet crates  
3. **facet-main**: Using facet from main branch with current PR's ks-* crates

## Installation

From the `facet2/outside-workspace/measure-bloat` directory:

```bash
cargo build --release
```

## Usage

### Test individual components

```bash
# Test ks-facet with current facet PR
cargo run -- test ks-facet facet-pr

# Test ks-facet with facet main branch
cargo run -- test ks-facet facet-main

# Test ks-serde with serde ecosystem
cargo run -- test ks-serde serde

# Test individual benchmark components
cargo run -- test json-benchmark facet-pr
cargo run -- test pretty-benchmark serde
cargo run -- test core-benchmark facet-main

# Debug TOML transformation
cargo run -- test debug-toml facet-main
```

### Run full comparison

```bash
# Full comparison across all variants
cargo run -- compare
```

This will:
1. Measure `ks-facet` with both `facet-pr` and `facet-main` variants
2. Measure `ks-serde` with `serde` variant
3. Generate a comprehensive comparison report in `bloat-results/comparison_report.md`

## Measurement Components

### Main Targets
- **ks-facet**: Complete facet ecosystem including JSON read/write and pretty printing
  - Crates: `ks-facet`, `ks-mock`, `ks-types`, `ks-facet-json-read`, `ks-facet-json-write`, `ks-facet-pretty`
  - Variants: `facet-pr`, `facet-main`

- **ks-serde**: Complete serde ecosystem equivalent
  - Crates: `ks-serde`, `ks-mock`, `ks-types`, `ks-serde-json-read`, `ks-serde-json-write`, `ks-debug`
  - Variants: `serde`

### Benchmark Components
- **json-benchmark**: JSON read/write functionality
- **pretty-benchmark**: Pretty printing functionality  
- **core-benchmark**: Core library without format-specific features

## Measured Metrics

For each target and variant, the tool measures:

- **Binary Size**: Total file size and text section size (via `cargo-bloat`)
- **Build Time**: Complete build duration
- **LLVM Lines**: IR line counts per crate (via `cargo-llvm-lines`)
- **Top Functions**: Largest functions by compiled size
- **Crate Breakdown**: Size contribution per crate

## Current Implementation Status

✅ **Fully Implemented**
- [x] Project structure and CLI interface
- [x] JSON parsing from cargo-bloat output
- [x] LLVM lines analysis integration
- [x] Build time measurement
- [x] Multi-variant support with git branch switching
- [x] Cargo.toml patching for facet-main comparison
- [x] Build isolation using temporary workspaces
- [x] Comprehensive markdown report generation
- [x] Function-level and crate-level diff analysis

## Technical Implementation

### Dependency Management

The tool handles complex dependency scenarios through sophisticated Cargo.toml manipulation:

- **facet-pr**: Uses current workspace state (no changes needed)
- **facet-main**: Creates temporary workspace, patches all Cargo.toml files to use `git = "https://github.com/facet-rs/facet", branch = "main"` for facet dependencies
- **serde**: Uses current ks-serde implementations (no changes needed)

### Build Isolation

- Uses separate temporary directories for facet-main variant
- Copies entire workspace to `/tmp/measure-bloat-{PID}/outside-workspace/`
- Automatically cleans up temporary workspaces after measurement
- Prevents cargo cache interference between builds

### Output Format

The tool generates structured analysis including:

- Summary comparison table with deltas between variants
- Top functions by compiled size for each variant
- LLVM IR line counts per crate
- Detailed PR vs main branch analysis
- Function-level and crate-level diff breakdowns

## Example Output

```
🧪 Testing component: ks-facet with variant: facet-pr
📏 Measuring target: ks-facet with variant: facet-pr
📦 Crates to measure: ["ks-facet", "ks-mock", "ks-types", "ks-facet-json-read", "ks-facet-json-write", "ks-facet-pretty"]
✅ Using current facet PR state (no changes needed)
⏱️  cargo bloat took: 3.799s
⏱️  cargo llvm-lines took: 12.234s
✅ Build complete

📊 Results:
   File size: 2.07 MiB
   Text section size: 831.59 KiB
   Build time: 15.42s
   Total LLVM lines: 12,847
   
   Top 5 functions:
   1. ks_facet_json_read::facet_deserialize::deserialize_wip: 21.0 KiB
   2. ks_facet_pretty::facet_pretty::printer::PrettyPrinter::format_peek_internal: 14.6 KiB
   3. facet_json::facet_serialize::serialize_iterative: 14.0 KiB
   4. facet_deserialize::facet_deserialize::StackRunner<C,I>::value: 10.7 KiB
   5. ariadne::write::<impl ariadne::Report<S>>::write_for_stream: 15.5 KiB
```

## Dependencies

Install required tools:
```bash
cargo install cargo-bloat cargo-llvm-lines
```

## Design Notes

### Removed Features

- **Plan subcommand**: Initially included a `plan` subcommand that would print the implementation roadmap. This was removed because the README already serves this purpose better - it's more discoverable, easier to maintain, and doesn't require running the tool to see the project status.

### JSON Output Parsing

Uses `cargo bloat --message-format json` for reliable structured data:

```json
{
  "file-size": 8027880,
  "text-section-size": 2127320,
  "functions": [
    {
      "crate": "ks_facet_pretty",
      "name": "facet_pretty::printer::PrettyPrinter::format_peek_internal", 
      "size": 25656
    }
  ]
}
```

### Error Handling

- Graceful degradation when individual measurements fail
- Continues with other crates if one fails LLVM lines analysis
- Automatic cleanup of temporary workspaces even on errors
- Detailed error context for debugging build issues