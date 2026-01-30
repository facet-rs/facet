# AFL Fuzzing for facet-reflect2

This directory contains an AFL fuzzing harness for facet-reflect2.

## Directory Structure

- `target/` - AFL-instrumented builds (used by `cargo afl build`)
- `target-standalone/` - Standalone builds for crash reproduction
- `target-cov/` - Coverage-instrumented builds
- `target-miri/` - Miri builds
- `out/` - AFL output (corpus, crashes, etc.)

These are separate to avoid AFL and non-AFL builds overwriting each other.

## The `standalone` Feature

The `standalone` feature builds without AFL integration. This is used for:

1. **Reproducing crashes** - Feed crash files to the binary directly
2. **Running under Miri** - Detect undefined behavior
3. **Generating coverage reports** - LLVM source coverage

Without this feature, the binary expects to run inside AFL's fork server.

## Common Tasks

```bash
just fuzz           # Start fuzzing
just resume         # Resume previous session
just run <file>     # Reproduce a crash
just run-miri <file> # Run crash under Miri
just cov html       # Generate coverage report
just clean          # Remove all build artifacts
```

## When Fixing Bugs Found by Fuzzing

1. Minimize the crash: `just minimize out/default/crashes/id:...`
2. Run under Miri to understand the issue: `just run-miri minimized.bin`
3. Fix the bug in facet-reflect2
4. Verify the fix: `just run minimized.bin` (should no longer crash)
5. Keep the minimized input as a regression test if appropriate
