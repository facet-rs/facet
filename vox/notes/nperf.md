# Profiling vox benches with not-perf (`nperf`)

## What `nperf` is

[koute/not-perf](https://github.com/koute/not-perf) — single-binary Rust
sampling profiler. Reads/writes its own format (not `perf.data`). Online
unwinding by default, so post-processing is fast even with deep stacks.
Built locally at `~/bearcove/not-perf/target/release/nperf`.

## Why we picked it over `perf`

`perf record -g --call-graph dwarf` ballooned the capture to 583 MB for
3 seconds and `perf report` then took ~30 seconds in addr2line on broken
build-id paths. Dropping the dwarf callgraph fixes the data size, but we
lose the caller chain we actually want. `nperf` keeps the call stacks
and ships a small file.

## The "wrong PID" gotcha (read this first)

`pgrep -f rpc-6645301d2ddf042f | head -1` returns the **shell wrapper**
process whose command line literally contains the bench-binary name,
*before* it returns the real bench. Attaching `nperf` to the wrapper
gives you 0–8 samples and a wild goose chase across `--event-source`
and `--offline`.

Always anchor the regex to the start of the binary path:

```sh
BENCH_PID=$(pgrep -f '^target/release/deps/rpc-664' | head -1)
```

Verify before recording:

```sh
ps -L -p "$BENCH_PID"   # should print the bench, not 'zsh'
```

If you see `zsh`, you're attached to the wrapper. Re-run the pgrep with
a tighter pattern.

## Standard workflow

Set `VOX_JIT_PERF=1` so vox-jit emits `/tmp/jit-<pid>.dump` (mono
jitdump format — same one perf consumes). `nperf` reads it via
`--jitdump`.

```sh
# 1. clean up
pkill -9 -f 'target/release/deps/rpc' 2>/dev/null
rm -f /tmp/nperf.dat /tmp/jit-*.dump /tmp/bench.log

# 2. start the bench in the background. DIVAN_MIN_TIME a few seconds
#    longer than the nperf -l window so the bench is still busy when
#    nperf stops.
( DIVAN_MIN_TIME=20 VOX_JIT_PERF=1 \
  target/release/deps/rpc-6645301d2ddf042f \
  --bench 'rpc::mem::jit::echo_gnarly' \
  > /tmp/bench.log 2>&1 & )

# 3. wait for the binary to actually exec (not just for the wrapper to
#    fork), then attach.
sleep 3
BENCH_PID=$(pgrep -f '^target/release/deps/rpc-664' | head -1)
echo "PID=$BENCH_PID"

~/bearcove/not-perf/target/release/nperf record \
  -p "$BENCH_PID" \
  -F 999 \
  -l 12 \
  -o /tmp/nperf.dat
```

Defaults that worked: `hw_cpu_cycles` event source, online unwinding.
About 1k samples per second of bench time. Expect a `Lost N events!`
warning with single-digit N — that's fine, the ring buffer occasionally
wraps.

## Sanity check before reporting

```sh
ls -lh /tmp/nperf.dat                       # ~1.5 MB per recorded second
~/bearcove/not-perf/target/release/nperf metadata /tmp/nperf.dat | head
```

If sample count is < 100 per second, something's wrong. The usual
suspects are wrong PID (see above) or the bench finished before nperf
attached.

## Subcommands worth knowing

| Subcommand     | Output                                     |
|----------------|--------------------------------------------|
| `record`       | Captures samples to `nperf.dat`.           |
| `metadata`     | One-line JSON summary of the capture.      |
| `csv`          | **Time-series**, not per-function. `Timestamp,Samples`. Useful for "did the bench plateau" checks, not for "where's the time going." |
| `collate`      | Folded stacks (`frame1;frame2;... count`). Pipe to flamegraph.pl, or aggregate in awk for a top-N report. |
| `flamegraph`   | SVG flamegraph directly.                   |
| `trace-events` | Chrome `chrome://tracing` JSON.            |

## Headless top-N report

`nperf` doesn't ship a `perf report --stdio` equivalent. We synthesize
one from `collate`:

```sh
JITDUMP=$(\ls /tmp/jit-*.dump | head -1)   # \ls strips zsh's color codes that leak into $()
~/bearcove/not-perf/target/release/nperf collate \
  --jitdump "$JITDUMP" \
  --merge-threads \
  /tmp/nperf.dat 2>/dev/null > /tmp/stacks.folded

# Top leaf-function self-time:
awk '{
  n=split($0,a," "); cnt=a[n];
  line=$0; gsub(/ [0-9]+$/,"",line);
  n2=split(line,b,";"); leaf=b[n2];
  tot[leaf]+=cnt; sum+=cnt
} END {
  for(k in tot) printf "%6d  %s\n", tot[k], k;
  print "TOTAL " sum > "/dev/stderr"
}' /tmp/stacks.folded | sort -rn | head -30
```

The folded format: each line is `frame_outer;frame...;frame_leaf
count`. Lines are unique per stack, not per leaf — same leaf appears
many times across distinct stacks. The awk above sums.

For caller chains of a specific function:

```sh
grep -F 'serialize_peek_inner' /tmp/stacks.folded | head -20
```

That gives you all the stacks that pass through `serialize_peek_inner`
along with their sample counts. Eyeball the parent frames to find who
called it.

## JIT'd code symbolication

`vox-jit` writes the mono jitdump to `/tmp/jit-<pid>.dump`. The
`record_load` calls in `rust/vox-jit/src/jitdump.rs` happen at compile
time. Pass `--jitdump /tmp/jit-<pid>.dump` to `collate`/`csv`/etc., and
JIT'd functions appear by their generated name (e.g.
`vox_encode__GnarlyPayload,__5`) instead of as raw addresses.

If you don't pass `--jitdump`, JIT'd frames show as
`0x000056... [unknown]`.

## libc (and other system-library) symbols

Stock `nperf collate` symbolicates exported `dynsym` entries in
system libraries (so you'll see `malloc`, `__libc_free`), but
non-exported internals like `__memcpy_avx512_unaligned_erms`,
`_int_malloc`, etc., come back as raw `0x00007F...` addresses inside
`[libc.so.6]`. That's where the actual time often lives — memcpy
variants alone can be ~8% of a hot bench.

Debian ships the detached debug info, but only by build-id, not at the
path nperf looks for:

```sh
sudo apt install libc6-dbg                  # already installed on this box
# debug file lives at:
ls /usr/lib/debug/.build-id/<2>/<rest>.debug
```

`nperf` does *not* follow `.build-id` even when given
`--debug-symbols /usr/lib/debug`. It logs warnings like
`Missing external debug symbols for '/usr/lib/x86_64-linux-gnu/...':
'<truncated>.debug'` and falls back to addresses.

Symlinking the debug file under the original library name does not help
for collate output either — same warning, same raw addresses.

**Practical workflow: post-resolve the top offenders with `addr2line`.**
Before the bench exits, snapshot `/proc/<PID>/maps` so you have the
ASLR-randomized load base:

```sh
cp /proc/$BENCH_PID/maps /tmp/proc-maps.txt
grep libc /tmp/proc-maps.txt | head -1
# 7ff81433e000-7ff814366000 r--p 00000000 ... libc.so.6
#                ^^^^^^^^^^^^ this is the load base
```

Then resolve any unresolved address `0x7FF8...` to a symbol:

```sh
DEBUG=/usr/lib/debug/.build-id/$(readelf -n /lib/x86_64-linux-gnu/libc.so.6 \
  | awk '/Build ID/ {print substr($3,1,2) "/" substr($3,3) ".debug"}')
BASE=0x7ff81433e000   # from /proc/<pid>/maps, libc.so.6's first segment
ADDR=0x00007FF8144B9A4F
addr2line -fipe "$DEBUG" $(printf '0x%x\n' $((ADDR - BASE)))
# __memcpy_avx512_unaligned_erms at .../memmove-vec-unaligned-erms.S:266
```

Resolve a batch in one shot:

```sh
for ADDR in 0x7FF8144B9A4F 0x7FF8144B9C66 0x7FF8144B9C5F; do
  off=$(printf '0x%x' $((ADDR - BASE)))
  addr2line -fipe "$DEBUG" $off
done
```

Save yourself time: the addresses that cluster within ~256 bytes of
each other are almost always inside one big asm helper (memcpy /
memmove / memset variant). Resolve the first one, you have all of them.

## Color-code leakage

`zsh` (and modern `ls`) emit ANSI color codes even into command
substitution. They sneak into `$JITDUMP` and break the path. Use `\ls`
(disables the alias) or pipe through `sed 's/\x1b\[[0-9;]*m//g'`. The
symptom is `failed to open jitdump "\u{1b}[36m/tmp/\u{1b}[0mjit-...`.

## Caveats

- `nperf record` attaches to a **single PID's threads**. It does not
  follow forks. Divan's `bench_local` runs synchronously on the calling
  thread, so this is fine for the rpc benches; if you ever switch to
  multi-threaded benches (or `bench_with_values` etc.), check
  `ls /proc/$PID/task/` to confirm where the work actually happens.
- The `nwind::address_space` `Duplicate PT_LOAD matches` warnings during
  record/collate are harmless — they're about ELF segment overlap in
  the binary itself, not your samples.
- File format is **not** compatible with `perf report` /
  `perf inject --jit`. If you need a `perf.data`, fall back to `perf
  record -k mono` (see existing `PERF_BUILDID_DIR=/tmp perf record -k
  mono ... && perf inject --jit ...` flow).
