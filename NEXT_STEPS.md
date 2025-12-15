# Next Steps for Benchmark Infrastructure

## Current State (WORKING!)
- ✅ 12/16 benchmarks ported to meta-harness (75%)
- ✅ 120 functions generated
- ✅ All compile and run
- ✅ cargo xtask gen-benchmarks works

## Issue
bench-report.sh runs old benchmarks (vs_format_json, gungraun_jit).
Should run unified_benchmarks instead.

## Problem
unified_benchmarks.rs has BOTH divan and gungraun in one file.
But Cargo.toml has `harness = false` so only gungraun runs!

## Solution
Split generator to create TWO files:
1. **unified_benchmarks_divan.rs** - Only divan benchmarks (harness = true)
2. **unified_benchmarks_gungraun.rs** - Only gungraun benchmarks (harness = false)

## Implementation Plan

### 1. Modify xtask/src/gen_benchmarks.rs:
```rust
pub fn generate_benchmarks(
    kdl_path,
    divan_output, // unified_benchmarks_divan.rs
    gungraun_output // unified_benchmarks_gungraun.rs
) {
    // Split into two functions:
    generate_divan_benchmarks()  // Only #[divan::bench] functions
    generate_gungraun_benchmarks()  // Only #[gungraun::library_benchmark] + main!()
}
```

### 2. Update xtask/src/main.rs:
```rust
let divan_path = workspace_root.join("facet-json/benches/unified_benchmarks_divan.rs");
let gungraun_path = workspace_root.join("facet-json/benches/unified_benchmarks_gungraun.rs");
gen_benchmarks::generate_benchmarks(&kdl_path, &divan_path, &gungraun_path)
```

### 3. Update facet-json/Cargo.toml:
```toml
[[bench]]
name = "unified_benchmarks_divan"
harness = false  # Uses divan

[[bench]]
name = "unified_benchmarks_gungraun"
harness = false  # Uses gungraun
```

### 4. Update scripts/bench-report.sh:
```bash
# OLD:
cargo bench --bench vs_format_json --features cranelift
cargo bench --bench gungraun_jit --features cranelift

# NEW:
cargo bench --bench unified_benchmarks_divan --features cranelift
cargo bench --bench unified_benchmarks_gungraun --features cranelift
```

### 5. Update parse_bench.py:
Parser already handles both formats, just reads different files.
No changes needed if file names match expected patterns.

## Benefits
- ✅ Both harnesses run from one definition
- ✅ Script works end-to-end
- ✅ HTML report has all data
- ✅ No manual duplication

## Current Workaround
Can run manually:
```bash
cargo bench --bench unified_benchmarks --features cranelift  # Runs gungraun only
# Divan benchmarks exist but don't run (harness = false)
```
