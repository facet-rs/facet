---
name: profiling
description: Profile code performance using callgrind and valgrind with nextest integration for analyzing instruction counts, cache behavior, and identifying bottlenecks
---

# Profiling with Valgrind, Callgrind, and Nextest

The facet project has pre-configured valgrind integration for debugging crashes, memory leaks, and performance profiling.

## Quick Usage

```bash
# Run test under valgrind (memory errors + leaks)
cargo nextest run --profile valgrind -p PACKAGE TEST_FILTER

# Run test under callgrind (profiling)
valgrind --tool=callgrind --callgrind-out-file=callgrind.out \
  cargo nextest run --no-fail-fast -p PACKAGE TEST_FILTER

# Analyze callgrind output
callgrind_annotate callgrind.out
# or with GUI
kcachegrind callgrind.out  # Linux
qcachegrind callgrind.out  # macOS
```

## Nextest Valgrind Profile

The project has a pre-configured valgrind profile in `.config/nextest.toml`:

### Configuration

```toml
[scripts.wrapper.valgrind]
# Leak checking configuration
command = 'valgrind --leak-check=full --show-leak-kinds=all --errors-for-leak-kinds=definite,indirect --error-exitcode=1'

[profile.valgrind]
# Apply to all tests on Linux
platform = 'cfg(target_os = "linux")'
filter = 'all()'
run-wrapper = 'valgrind'
```

**What it does:**
- `--leak-check=full` - Show details for each leak
- `--show-leak-kinds=all` - Show all leak types for diagnostics
- `--errors-for-leak-kinds=definite,indirect` - Only fail on real leaks (not "still reachable")
- `--error-exitcode=1` - Exit with code 1 if errors found

### Usage

```bash
# Run specific test
cargo nextest run --profile valgrind -p facet-format-json test_simple_struct

# Run all tests in a file
cargo nextest run --profile valgrind -p facet-format-json --test jit_deserialize

# Run with filter
cargo nextest run --profile valgrind -p facet-json booleans
```

**Benefits:**
- ✅ Automatic configuration - no manual valgrind commands
- ✅ Consistent flags across team
- ✅ Integrated with nextest filtering
- ✅ Clean, formatted output

## Profiling with Callgrind

Callgrind is a valgrind tool for profiling instruction counts and function call graphs.

### Basic Profiling

```bash
# Profile a specific test
valgrind --tool=callgrind \
  --callgrind-out-file=callgrind.out \
  cargo nextest run --no-fail-fast -p PACKAGE TEST_NAME

# Analyze output
callgrind_annotate callgrind.out
```

### Advanced Options

```bash
# Collect cache simulation data (slower but more detailed)
valgrind --tool=callgrind \
  --cache-sim=yes \
  --branch-sim=yes \
  --callgrind-out-file=callgrind.out \
  cargo nextest run --no-fail-fast -p PACKAGE TEST_NAME

# Focus on specific function
valgrind --tool=callgrind \
  --toggle-collect=main \
  --callgrind-out-file=callgrind.out \
  cargo nextest run --no-fail-fast -p PACKAGE TEST_NAME

# Compress output (can get large)
valgrind --tool=callgrind \
  --compress-strings=yes \
  --compress-pos=yes \
  --callgrind-out-file=callgrind.out.gz \
  cargo nextest run --no-fail-fast -p PACKAGE TEST_NAME
```

### Analyzing Callgrind Output

#### Command Line (callgrind_annotate)

```bash
# Full report
callgrind_annotate callgrind.out

# Focus on specific functions
callgrind_annotate --include='facet::' callgrind.out

# Show only top functions
callgrind_annotate --auto=yes --threshold=1 callgrind.out

# Compare two runs
callgrind_annotate --diff callgrind.old.out callgrind.new.out
```

**Reading the output:**
```
Ir                                     # Instruction reads (total)
I1mr                                   # L1 instruction cache misses
ILmr                                   # Last-level instruction cache misses
Dr                                     # Data reads
Dw                                     # Data writes
D1mr, D1mw                            # L1 data cache read/write misses
DLmr, DLmw                            # Last-level data cache read/write misses

--------------------------------------------------------------------------------
Ir               file:function
--------------------------------------------------------------------------------
1,234,567 (45%)  facet_format_json::deserialize
  987,654 (35%)  facet_format::parse_value
  ...
```

#### GUI (KCachegrind/QCachegrind)

Install:
```bash
# Linux
sudo apt install kcachegrind

# macOS
brew install qcachegrind

# Windows (WSL)
sudo apt install kcachegrind
```

Launch:
```bash
kcachegrind callgrind.out   # Linux
qcachegrind callgrind.out   # macOS
```

**GUI features:**
- Call graph visualization
- Flamegraph-like views
- Source code annotation (if debug symbols available)
- Caller/callee relationships
- Multiple metrics (instructions, cache misses, branches)

## Profiling Benchmarks

The generated benchmark tests (from `benchmarks.kdl`) can be profiled:

### 1. As Tests (Recommended for Callgrind)

```bash
# Profile a benchmark test under callgrind
valgrind --tool=callgrind \
  --callgrind-out-file=callgrind_simple_struct.out \
  cargo nextest run --profile valgrind -p facet-json test_simple_struct

# Analyze
callgrind_annotate callgrind_simple_struct.out
```

**Why use tests:**
- Single iteration = cleaner callgrind output
- No benchmark harness overhead
- Easier to focus on hot path
- Faster to run

### 2. As Benchmarks (For Realistic Instruction Counts)

The benchmark harness (gungraun) already uses valgrind internally:

```bash
# Run gungraun benchmark (uses callgrind automatically)
cargo bench --bench unified_benchmarks_gungraun --features jit simple_struct

# Check output in bench-reports/gungraun-*.txt
```

**gungraun automatically collects:**
- Instructions executed
- Estimated cycles
- L1/LL cache hits
- RAM hits
- Total read/write operations

This data appears in `bench-reports/perf/RESULTS.md`.

## Common Profiling Workflows

### Debug a Crash

```bash
# 1. Run under valgrind to find memory error
cargo nextest run --profile valgrind -p PACKAGE TEST_NAME

# 2. Read valgrind output for exact error location
# Example: "Invalid read of size 8 at 0x123456"

# 3. Fix the bug

# 4. Verify fix
cargo nextest run -p PACKAGE TEST_NAME
```

### Find Performance Bottleneck

```bash
# 1. Profile with callgrind
valgrind --tool=callgrind \
  --callgrind-out-file=profile.out \
  cargo nextest run --no-fail-fast -p facet-json test_booleans

# 2. Analyze
callgrind_annotate --auto=yes profile.out | head -30

# 3. Identify hot functions (high instruction counts)

# 4. Optimize hot functions

# 5. Re-profile and compare
valgrind --tool=callgrind \
  --callgrind-out-file=profile_after.out \
  cargo nextest run --no-fail-fast -p facet-json test_booleans

callgrind_annotate --diff profile.out profile_after.out
```

### Optimize Tier-2 JIT

```bash
# 1. Check RESULTS.md for slow benchmarks
grep "⚠" bench-reports/perf/RESULTS.md

# 2. Profile the slow benchmark test
valgrind --tool=callgrind \
  --callgrind-out-file=jit_profile.out \
  cargo nextest run --profile valgrind -p facet-json test_long_strings --features jit

# 3. Analyze with GUI for visual call graph
kcachegrind jit_profile.out

# 4. Look for:
#    - Helper function calls in tight loops
#    - Redundant alignment checks
#    - Allocation hot spots

# 5. Optimize based on findings

# 6. Verify with benchmarks
cargo xtask bench long_strings
```

### Compare Before/After Optimization

```bash
# Before
git checkout main
valgrind --tool=callgrind --callgrind-out-file=before.out \
  cargo nextest run --no-fail-fast -p facet-json test_target

# After
git checkout my-optimization-branch
valgrind --tool=callgrind --callgrind-out-file=after.out \
  cargo nextest run --no-fail-fast -p facet-json test_target

# Compare
callgrind_annotate --diff before.out after.out
```

## Interpreting Valgrind Output

### Memory Error Example

```
==12345== Invalid read of size 8
==12345==    at 0x123456: facet_format_json::parse_number (parse.rs:42)
==12345==    by 0x234567: facet_format_json::deserialize (lib.rs:123)
==12345==  Address 0x789abc is 0 bytes after a block of size 16 alloc'd
==12345==    at 0x345678: alloc (alloc.rs:88)
==12345==    by 0x456789: Vec::push (vec.rs:1234)
```

**Translation:**
- Reading 8 bytes from invalid address
- Happened in `parse_number` at line 42
- Address is just past end of 16-byte allocation
- **Fix:** Check bounds before reading, or fix off-by-one error

### Leak Example

```
==12345== 128 bytes in 1 blocks are definitely lost in loss record 1 of 10
==12345==    at 0x123456: malloc (vg_replace_malloc.c:299)
==12345==    by 0x234567: alloc (alloc.rs:88)
==12345==    by 0x345678: Box::new (boxed.rs:123)
==12345==    by 0x456789: setup_jit (jit.rs:456)
```

**Translation:**
- 128 bytes allocated but never freed
- Allocated in `setup_jit` function
- **Fix:** Ensure cleanup/Drop implementation

### Cachegrind Output Example

```
Ir               I1mr  ILmr  Dr        D1mr   DLmr   Dw        D1mw   DLmw
--------------------------------------------------------------------------------
1,234,567        123   45    456,789   234    12     123,456   67     8   facet::deserialize
  987,654        98    32    345,678   189    9      98,765    43     5   - facet::parse_value
  234,567        23    10    98,765    45     2      23,456    12     1   - facet::parse_string
```

**Key metrics:**
- `Ir` - Instructions executed (most important for optimization)
- `D1mr/D1mw` - L1 data cache misses (indicates poor locality)
- `DLmr/DLmw` - Last-level cache misses (very expensive)

**Optimization targets:**
1. High `Ir` count = time-consuming function
2. High `D1mr` = poor data locality, consider restructuring
3. High `DLmr` = main memory accesses, critical to optimize

## Profiling Flags

### Valgrind (Memory Debugging)

```bash
--leak-check=full          # Detailed leak info
--show-leak-kinds=all      # Show all leak types
--track-origins=yes        # Track uninitialized values (slower)
--verbose                  # More diagnostic info
--log-file=valgrind.log    # Save output to file
```

### Callgrind (Profiling)

```bash
--callgrind-out-file=FILE  # Output file (default: callgrind.out.<pid>)
--cache-sim=yes            # Simulate cache behavior
--branch-sim=yes           # Simulate branch prediction
--collect-jumps=yes        # Collect jump information
--dump-instr=yes           # Dump instruction info
--compress-strings=yes     # Compress output (smaller files)
```

### Cargo Nextest

```bash
--no-fail-fast            # Continue running after first failure
--profile valgrind        # Use valgrind profile from nextest.toml
--test-threads=1          # Run single-threaded (better for profiling)
```

## Tips and Tricks

### Speed Up Profiling

1. **Profile in release mode** (but keep debug symbols):
   ```bash
   # Add to Cargo.toml
   [profile.release]
   debug = true
   ```

2. **Use `--no-fail-fast` to avoid stopping early**

3. **Filter to specific tests** - don't profile everything at once

4. **Disable address randomization** for reproducible runs:
   ```bash
   setarch $(uname -m) -R valgrind --tool=callgrind ...
   ```

### Read Callgrind Data Programmatically

```python
# Example: Parse callgrind output for automation
def parse_callgrind(filename):
    import re
    costs = {}
    with open(filename) as f:
        for line in f:
            if m := re.match(r'(\d+)\s+(.+)', line):
                cost, func = m.groups()
                costs[func] = int(cost)
    return costs

# Compare two profiles
before = parse_callgrind('before.out')
after = parse_callgrind('after.out')

for func in before:
    if func in after:
        delta = after[func] - before[func]
        percent = (delta / before[func]) * 100
        if abs(percent) > 5:  # More than 5% change
            print(f"{func}: {percent:+.1f}% ({delta:+,} instructions)")
```

## Don't Do This

❌ Run valgrind without nextest profile - inconsistent flags
❌ Profile debug builds - too slow and unrepresentative
❌ Ignore "still reachable" leaks in FFI code - sometimes OK
❌ Profile with multiple test threads - non-deterministic results
❌ Forget to clean between profiling runs - stale data

## Do This Instead

✅ Use `--profile valgrind` for memory debugging
✅ Use callgrind for performance profiling
✅ Profile release builds with debug symbols
✅ Focus on hot paths (high `Ir` counts)
✅ Compare before/after with `--diff`
✅ Use GUI tools (kcachegrind) for complex call graphs

## Files and Locations

```
.config/nextest.toml         # Valgrind profile configuration
callgrind.out.*              # Callgrind output files (gitignored)
bench-reports/gungraun-*.txt # Gungraun output (includes instruction counts)
```

## Troubleshooting

### Valgrind complains about "unrecognized instruction"
- Update valgrind: `sudo apt update && sudo apt install valgrind`
- Or use `--vex-iropt-register-updates=allregs-at-mem-access`

### Callgrind output is huge
- Use `--compress-strings=yes --compress-pos=yes`
- Or filter to specific functions with `--toggle-collect=function_name`

### Profile doesn't match benchmark results
- Ensure you're profiling the same code path
- Check if JIT compilation is cached (use setup functions in gungraun)
- Profile release build, not debug

### Can't open callgrind file in GUI
- Check file permissions
- Ensure file isn't corrupted (run `callgrind_annotate` first)
- Try different viewer (kcachegrind vs qcachegrind)

## See Also

- Valgrind manual: https://valgrind.org/docs/manual/manual.html
- Callgrind manual: https://valgrind.org/docs/manual/cl-manual.html
- Nextest wrapper scripts: https://nexte.st/docs/configuration/wrapper-scripts/
- KCachegrind handbook: https://docs.kde.org/stable5/en/kcachegrind/
- Project nextest config: `.config/nextest.toml`
- Benchmark debugging: See `benchmarking.md`
