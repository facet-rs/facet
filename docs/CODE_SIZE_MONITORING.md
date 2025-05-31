# Code Size Monitoring

This document explains how to monitor and manage code size in the Facet project, including generated
code, binary sizes, and compile-time metrics.

## Why Monitor Code Size?

Code size impacts multiple aspects of the project:

- **Compile times**: Larger generated code and excessive monomorphization can significantly slow
  down compilation
- **Binary size**: Affects deployment, download times, and resource usage
- **Maintenance**: Larger generated code is harder to debug and maintain
- **Performance**: Code bloat can negatively impact runtime performance through cache misses and
  instruction fetch overhead

## Local Measurement Tools

### Prerequisites

Install the required tools:

```bash
just install-size-tools
```

This installs:
- `cargo-bloat`: Analyzes binary size by crate and function
- `cargo-llvm-lines`: Measures LLVM IR line count (correlates with compile time)
- `cargo-binutils`: Provides binary inspection tools

### Measuring Code Size Locally

Run the following command to generate a complete code size report:

```bash
just code-size
```

This creates reports in the `code-size-data/` directory, including:
- `generated-sizes.md`: Size metrics for generated files
- `binary-sizes.md`: Size of compiled libraries (`.rlib` files)

### Comparing with Main Branch

To compare your current branch with the main branch:

```bash
just code-size-diff
```

This will:
1. Measure generated code in your current branch
2. Temporarily switch to the main branch and measure there
3. Switch back to your branch
4. Generate a comparison report showing size differences

## GitHub CI Integration

The project includes a GitHub Actions workflow (`code-size.yml`) that automatically:

1. Runs on all PRs and main branch commits
2. Measures code size metrics
3. Compares PR changes against the main branch
4. Posts a comment on PRs with significant size changes
5. Archives historical size data for trend analysis

### Understanding CI Results

When a PR is submitted, the workflow will comment with:

1. **Code Size Comparison**: Shows changes in generated code size
2. **Binary Sizes**: Current binary size of compiled libraries
3. **Dependency Bloat Analysis**: Summary of space usage by dependencies

A sample PR comment might look like:

```
## Code Size Comparison
| File | Before (bytes) | After (bytes) | Difference | % Change |
|------|---------------|--------------|------------|----------|
| facet-core/src/tuples_impls.rs | 250000 | 255000 | +5000 | +2.00% |
| facet-reflect/src/vtable.generated.rs | 120000 | 118000 | -2000 | -1.67% |

## Binary Sizes
| Crate | Size (bytes) |
|-------|-------------|
| facet_core | 1250000 |
| facet_reflect | 980000 |
```

## Handling Code Size Regressions

When significant code size increases are detected:

1. **Review generated code changes**: Check what's causing the size increase
2. **Consider algorithmic improvements**: Look for ways to generate more efficient code
3. **Examine templates**: Templates that generate code might need optimization
4. **Optimize macros**: Refine macros to produce less verbose output
5. **Dependency review**: Check if new dependencies are causing bloat

### Known Size Hotspots

These files typically account for the majority of generated code:
- `facet-core/src/tuples_impls.rs`
- `facet-reflect/src/vtable.generated.rs`

### Setting Size Budgets

As a general guideline:
- Aim to keep individual generated files under 1MB
- Total generated code should grow sublinearly with feature additions
- Binary size increases should be proportional to new functionality

## Historical Tracking

Code size history is tracked on the main branch, with data points stored in:
- `.code-size-history/` directory (size snapshots per commit)
- CI artifacts (retained for 90 days)

To analyze long-term trends, examine the historical data points which include:
- Total generated code size
- Total binary size
- Key metrics for individual components
