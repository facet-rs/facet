# TODO: Port All Benchmarks to Meta-Harness

## Current Status: 12/16 COMPLETE! (75%)

**Ported (12/16):**

**Micro (3/3):**
- ✅ simple_struct
- ✅ single_nested_struct
- ✅ simple_with_options

**Arrays (6/6):**
- ✅ booleans
- ✅ integers
- ✅ floats
- ✅ short_strings
- ✅ long_strings
- ✅ escaped_strings

**Complex (3/3):**
- ✅ nested_structs
- ✅ hashmaps
- ✅ options

**Remaining (4/16):**

### Realistic Benchmarks (2)
- [ ] **twitter** - Uses `corpus/twitter.json.br` (brotli compressed)
  * Type: TwitterResponseSparse
  * Needs: Decompress and embed JSON or reference file

- [ ] **canada** - Uses `corpus/canada.json.br` (brotli compressed)
  * Type: Canada
  * Needs: Decompress and embed JSON or reference file

### Array Benchmarks with Generated Data (6)
All use `make_data()` functions that generate Vec<T>:

- [ ] **integers** - Vec<u64>, 1000 items
  * Generate once: `(0..1000).map(|i| i * 12345678901234)`
  * Serialize to JSON, embed in KDL

- [ ] **floats** - Vec<f64>, 1000 items
  * Generate once: `(0..1000).map(|i| i as f64 * 1.23456789)`

- [ ] **short_strings** - Vec<String>, 1000 items, 10 chars each
  * Generate once: `(0..1000).map(|i| format!("str_{:06}", i))`

- [ ] **long_strings** - Vec<String>, 100 items, 1000 chars each
  * Generate once: `(0..100).map(|i| "x".repeat(1000) + &format!("{}", i))`

- [ ] **escaped_strings** - Vec<String>, 1000 items with escapes
  * Contains: `"\n", "\t", "\"", "\\"`

- [ ] **booleans** - Vec<bool>, 10000 items
  * Generate once: `(0..10000).map(|i| i % 2 == 0)`

### Complex Benchmarks (5)

- [ ] **nested_structs** - Vec<Outer> with 3-level deep nesting, 500 items
  * Type: Vec<Outer { id, inner: Inner { name, value, deep: Deep { flag, count } } }>
  * Generate 500 items

- [ ] **hashmaps** - HashMap<String, u64>, 1000 entries
  * Generate once: `(0..1000).map(|i| (format!("key_{}", i), i * 2))`

- [ ] **options** - Vec<MaybeData>, 500 items
  * Type: MaybeData { required: u64, optional_string: Option<String>, optional_number: Option<f64> }
  * Mix of Some/None values

- [ ] **flatten_2enums** - Complex flattened enum structure
  * Needs type extraction

- [ ] **flatten_4enums** - Complex flattened enum structure
  * Needs type extraction

## Approach Options

### Option 1: Pre-generate JSON Files
```bash
# Create a helper script
cargo run --example gen_bench_data
# Outputs facet-json/benches/corpus/*.json
# Reference in KDL with file paths or embed
```

### Option 2: Inline Generation in xtask
Modify `gen_benchmarks.rs` to support:
```kdl
benchmark name="integers" type="Vec<u64>" category="array" {
    data_generator {
        code "(0..1000).map(|i| i * 12345678901234).collect::<Vec<u64>>()"
    }
}
```

Then xtask evaluates the code, serializes to JSON, embeds in generated file.

### Option 3: Commit JSON Directly (Simple!)
Generate all JSON data once, commit to repo:
```
facet-json/benches/data/integers.json
facet-json/benches/data/floats.json
etc.
```

Reference in KDL, xtask reads and embeds.

## Recommended: Option 3 (Commit JSON)

**Pros:**
- Simple to implement
- Deterministic (same JSON every time)
- Easy to review
- Fast generation

**Cons:**
- Larger repo size (but benchmarks are dev-only)

## Next Steps

1. Create `gen_bench_data.rs` example that generates all JSON
2. Run it once, commit JSON files to `facet-json/benches/data/`
3. Add all benchmarks to `benchmarks.kdl` referencing data files
4. Modify `gen_benchmarks.rs` to support reading JSON from files
5. Regenerate unified_benchmarks.rs
6. Verify all 16 benchmarks work with both divan and gungraun

## Notes

- Vec<T> benchmarks will show JIT fallback (informative!)
- Flatten benchmarks may not be JIT-compatible (will error, also informative)
- Goal: Complete parity - every benchmark has both time + instructions
