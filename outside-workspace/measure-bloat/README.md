# measure-bloat

A Rust utility for measuring and comparing binary sizes and build times between different serialization ecosystems.

## Overview

This tool compares three different scenarios:

1. **serde-latest**: Using the latest serde ecosystem
2. **facet-pr**: Using facet from current PR/HEAD  
3. **facet-main**: Using facet from main branch (with PR's ks-* crates)

## Installation

From the `facet2/outside-workspace/measure-bloat` directory:

```bash
cargo build --release
```

## Usage

### Show the implementation plan

```bash
cargo run -- plan
```

### Test individual components

```bash
# Test JSON benchmark with current facet PR
cargo run -- test json-benchmark facet-pr

# Test pretty printing benchmark with serde
cargo run -- test pretty-benchmark serde

# Test core functionality with facet main branch
cargo run -- test core-benchmark facet-main
```

### Run full comparison (when implemented)

```bash
# Full comparison across all variants
cargo run -- compare

# Skip serde comparison during development
cargo run -- compare --skip-serde

# Skip facet-main comparison
cargo run -- compare --skip-main

# Specify output directory
cargo run -- compare --output ./results
```

## Measurement Targets

### 1. JSON Read/Write Benchmark
- **Facet crates**: ks-facet, ks-mock, ks-facet-json-read, ks-facet-json-write
- **Serde crates**: ks-serde, ks-mock, ks-serde-json-read, ks-serde-json-write

### 2. Pretty Printing Benchmark  
- **Facet crates**: ks-facet, ks-mock, ks-facet-pretty
- **Serde crates**: ks-serde, ks-mock, ks-debug-print

### 3. Core Library Size
- **Facet crates**: ks-facet, ks-mock
- **Serde crates**: ks-serde, ks-mock

## Current Status

✅ **Phase 1: Infrastructure**
- [x] Project structure created
- [x] Measurement targets defined
- [x] JSON parsing from cargo-bloat implemented
- [x] Basic command execution working

🚧 **Phase 2: Basic Measurements** (In Progress)
- [x] Single-target measurement for facet-pr
- [ ] LLVM lines analysis integration
- [ ] Build time measurement

🔜 **Phase 3: Multi-Variant Support** (Planned)
- [ ] Git branch switching for facet-main comparison
- [ ] Cargo.toml patching for mixed dependencies
- [ ] Build isolation between variants

🔜 **Phase 4: Serde Integration** (Planned)
- [ ] Serde-based variants of ks-* crates
- [ ] Equivalent benchmark implementations

🔜 **Phase 5: Reporting** (Planned)
- [ ] Markdown report generation
- [ ] Comparison tables and diffs
- [ ] GitHub Actions integration

## Example Output

```
🧪 Testing component: json-benchmark with variant: facet-pr
📏 Measuring target: json-benchmark with variant: facet-pr
📦 Crates to measure: ["ks-facet", "ks-mock", "ks-facet-json-read", "ks-facet-json-write"]
🚀 Would measure using current facet PR
⏱️  cargo bloat took: 3.799342791s
✅ cargo-bloat results:
   File size: 2165528 bytes
   Text section size: 851556 bytes
   Top 5 functions:
   1. ks_facet_json_read (facet_deserialize::deserialize_wip): 21456 bytes
   2. facet_deserialize (ariadne::write::<impl ariadne::Report<S>>::write_for_stream): 15924 bytes
   3. ks_facet_pretty (facet_pretty::printer::PrettyPrinter::format_peek_internal): 14932 bytes
   4. facet_json (facet_serialize::serialize_iterative): 14316 bytes
   5. facet_deserialize (facet_deserialize::StackRunner<C,I>::value): 10996 bytes
```

## Dependencies

- `cargo-bloat`: For binary size analysis
- `cargo-llvm-lines`: For LLVM IR line count analysis

Install with:
```bash
cargo install cargo-bloat cargo-llvm-lines
```

## Technical Notes

### Dependency Management Challenges

The tool needs to handle complex dependency scenarios:

1. **facet-main + PR ks-crates**: Use `[patch.crates-io]` to mix main branch facet with PR's ks-* crates
2. **Build isolation**: Use separate target directories or cargo clean to avoid cache interference  
3. **Git state management**: Use git stash/unstash or separate worktrees to switch branches safely

### JSON Output Format

The tool uses `cargo bloat --message-format json` for reliable parsing:

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

This is much more reliable than parsing text output and provides structured data for analysis.