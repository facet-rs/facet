# AFL Fuzzing for trame

## Setup

Install cargo-afl:

```bash
cargo install cargo-afl
```

On macOS, run the system config (needs sudo):

```bash
cargo afl system-config
```

## Running the Fuzzer

```bash
just fuzz
```

This builds three binaries and runs AFL with SAND (decoupled sanitizers):
- Native binary (fast, for coverage)
- ASAN+UBSAN binary (memory errors + undefined behavior)
- MSAN binary (uninitialized memory)

To resume a previous session:

```bash
just resume
```

## Reproducing Crashes

Crashes are saved in `out/default/crashes/`.

**Always run crashes under Miri first:**

```bash
just run-miri out/default/crashes/id:000000*
```

Miri provides precise diagnostics about what memory safety violation occurred
(use-after-free, uninitialized reads, invalid pointer arithmetic, etc.), while
a raw segfault gives you almost nothing. GDB backtraces show where it crashed,
not what went wrong.

If Miri doesn't catch it (e.g., logic errors, panics), run standalone:

```bash
just run out/default/crashes/id:000000*
```

With backtrace:

```bash
just run-bt out/default/crashes/id:000000*
```

## Minimizing Crash Inputs

To get a minimal reproducer:

```bash
just minimize out/default/crashes/id:000000*
```

For hangs (if `tmin` says "Target binary times out"):

```bash
cargo afl tmin -H -t 5000 -i out/default/crashes/id:000000* -o minimized.bin -- target/debug/trame-afl
```

## Source Coverage

Generate a coverage report from the fuzzer corpus:

```bash
just cov          # summary report
just cov html     # HTML report (opens in browser)
just cov uncovered  # show uncovered lines
```

## Cleaning Up

```bash
just clean
```
