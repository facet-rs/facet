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

Crashes are saved in `out/default/crashes/`. To reproduce one:

```bash
target/debug/facet-reflect2-afl < out/default/crashes/id:000000*
```

To see a backtrace:

```bash
RUST_BACKTRACE=1 target/debug/facet-reflect2-afl < out/default/crashes/id:000000*
```

To check multiple crashes at once:

```bash
for f in out/default/crashes/id:*; do
    echo "=== $f ==="
    target/debug/facet-reflect2-afl < "$f" 2>&1 | tail -3
done
```

## Minimizing Crash Inputs

To get a minimal reproducer:

```bash
cargo afl tmin -i out/default/crashes/id:000000* -o minimized.bin -- target/debug/facet-reflect2-afl
```

## Cleaning Up

```bash
just clean
```
