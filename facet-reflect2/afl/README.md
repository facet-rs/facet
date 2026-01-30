# AFL Fuzzing for facet-reflect2

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

```bash
just run out/default/crashes/id:000000*
```

With backtrace:

```bash
just run-bt out/default/crashes/id:000000*
```

Under Miri (catches undefined behavior, memory safety issues):

```bash
just run-miri out/default/crashes/id:000000*
```

## Minimizing Crash Inputs

To get a minimal reproducer:

```bash
just minimize out/default/crashes/id:000000*
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
