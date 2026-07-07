# LR loop in vix: measurement spike

Branch/worktree: `lr-loop-vix-baseline` at `/Users/amos/.paseo/worktrees/1t3lgrd0/spike-d-lr-loop`.

## Question

Can a parser's hottest LR shift/reduce loop live directly in vix today, as input
to the Snark lowering decision?

Short answer: not today. The exact requested shape, a dense 2D action table
indexed by `(state, token)`, is not expressible in vix yet because arrays have
`.push`, `.pop`, `.set`, and `.len`, but no dynamic `.get`/index operation. The
measured LR loop therefore uses the same LR automaton on both sides, but encodes
the action/goto table as nested `match` expressions. That makes the table gap a
headline finding and keeps the measurement focused on the loop, recursion, token
array, and parse-stack costs that vix can express today.

## Benchmark

Added binary: `vix/src/bin/lr_loop_bench.rs`.

Grammar: `E -> ID | E PLUS ID`, token stream `ID ("+" ID)* EOF`.

Runtime shape:

- Rust: `Vec<i64>` token stack plus parse stack, match-coded action/goto table,
  checksum is reduce count.
- vix LR: `[Int]` token stack, `[Int]` parse stack, recursive `parse(...)`,
  `.pop()` to read stack/token tops, `.push()` for shifts/gotos, checksum is
  reduce count.
- vix fresh-temporary control: generated nested expression
  `([0].push(...).pop()).1...`, measuring the current in-place molten reuse
  ceiling when receivers are not read from named bindings.

Focused verification:

```text
cargo nextest list -p vix -E 'binary(lr_loop_bench)'
cargo nextest run -p vix -E 'binary(lr_loop_bench)'
```

Result: 2 tests passed (`rust_and_vix_match_small_lr_stream`,
`fresh_temporary_array_control_matches_rust`).

## LR Results

Release build:

```text
cargo build --release -p vix --bin lr_loop_bench
```

Main same-size comparison, 10k tokens / 15k LR actions:

| lane | command shape | ns/action | checksum | factor vs Rust |
|---|---:|---:|---:|---:|
| Rust | `--terms 5000 --runs 10000 --mode rust` | 2.898 | 50,000,000 | 1.0x |
| vix interp, default molten mode | `--terms 5000 --runs 1 --mode vix-interp --molten-reuse` | 672,316.678 | 5,000 | 231,993x |
| vix JIT, default molten mode | `--terms 5000 --runs 1 --mode vix-jit --molten-reuse` | 3,779,842.308 | 5,000 | 1,304,293x |

The JIT lane is meaningfully different here, but in the wrong direction:
roughly 5.6x slower than the interpreter for this recursive/memo-heavy shape.

Large-N probe:

| run | result |
|---|---|
| Rust, 100k tokens / 150k actions, 1000 runs | 1.038 ns/action |
| vix interp, 100k tokens / 150k actions, forced-copy control | did not finish within a 120s alarm |

That timeout is already a lower bound of `>800,000 ns/action`, or `>770,789x`
against the one-run equivalent of the 100k-token Rust baseline. I did not run
the default molten-reuse 100k-token shape to completion; with today's named-read
reuse miss it is not a useful floor measurement.

## Fresh-Temporary Reuse Control

Command:

```text
./target/release/lr_loop_bench --mode array-control \
  --array-pushes 1024 --array-burst 32 --array-pops 16 --array-runs 100
```

1536 array ops per run, 100 runs:

| lane | ns/op | factor vs Rust | note |
|---|---:|---:|---|
| Rust `Vec` | 1.149 | 1.0x | reference stack ops |
| vix interp, forced copy | 204.621 | 178.1x | copy path |
| vix interp, reuse enabled | 77.520 | 67.5x | in-place reuse fires |
| vix JIT, forced copy | 432.585 | 376.5x | copy path |
| vix JIT, reuse enabled | 284.445 | 247.6x | in-place reuse fires |

This is not the LR loop. It is the control requested in the mid-flight intel:
fresh temporary receivers keep `refs == 1`, so the molten array reuse gate fires.
It shows a real reuse ceiling exists today: interpreter copy -> reuse is a 2.64x
win on this control, and the reuse-on interp floor is ~77.5 ns/array op.

## stax Decomposition

Profiled command:

```text
stax record -- ./target/release/lr_loop_bench \
  --terms 5000 --runs 1 --mode vix-interp --molten-reuse
stax flame -d 20 --threshold-pct 0.5 --run 3
stax top -n 25 --sort self --run 3
stax threads -n 20 --run 3
```

The profiled run printed `vix_interp_ns_per_action=1061445.486`; stax reported
3.532s total active CPU in the selected run. Active-time trunk:

| stack / frame | active share |
|---|---:|
| `lr_loop_bench::bench_vix` | 99.7% |
| `Machine::demand_i64 -> Driver::demand` | 95.6% |
| `weavy::task::Task::run_hosted` | 77.3% |
| `Driver::burst::{closure}` | 77.2% |
| `intern_molten_word` | 77.0% |
| `ValueStore::alloc_array_words` under `intern_molten_word` | 60.2% |
| `sha2::sha256::compress256` under allocation/canonical hashing | dominant leaf |
| `Driver::projection_memo_hit -> projection_candidate_key` | 14.2% |
| vix source parse/load | 3.9% |

Flat `top` agrees with the flame: `sha2::sha256::compress256` dominates self
time, followed by canonical store hashing / projection-candidate work. The main
thread had 3.531s CPU and 9.998s off-CPU in `stax threads`; I use active CPU
shares above for attribution.

## Attribution

| Cost | Evidence | Planned removal / mitigation | Ceiling if removed |
|---|---|---|---|
| Copy-path amplification from named stack/token reads | `resolve_binding` emits `MOLTEN_DUP` for molten binding reads, while array push/pop/set reuse gates require `refs == 1`; there is no Drop, so `let-ish` recursive rebinding misses reuse. The LR loop is therefore not measuring the molten reuse floor. | Consuming-move for `let x = x.push(v)` / rebind idioms on `molten-consume`; then Dup-Drop uniqueness / in-place reuse stencil from the reuse-analysis design. | Removes the O(N^2)-looking stack-copy amplification. Fresh-temporary control shows the current interp array-op floor is ~77.5 ns/op, ~8673x below the measured 10k LR interp ns/action. |
| Store allocation + canonical hashing | stax: 77.0% active under `intern_molten_word`, 60.2% under `ValueStore::alloc_array_words`, with SHA-256 compression as the dominant leaf. | In-place reuse avoids allocation/hash for hot stack updates; descriptor-driven/incremental hashing and planned hash changes help remaining store boundaries. | Removing only this trunk is a ~4.3x active-CPU ceiling on the profiled run; combined with consuming moves it changes the asymptotic behavior of stack updates. |
| Projection memo candidate construction | stax: 14.2% under `projection_memo_hit -> projection_candidate_key -> is_projectable_arg`. | Skip projection-candidate machinery for functions/args that cannot project, or cache the per-signature projectability decision. | At most ~1.16x by itself, but important once allocation/hashing drops. |
| Recursive loop / memo-boundary frame traffic | vix has no loop construct here; multi-statement match arms had to become helper functions, and recursion crosses memo/task machinery. JIT is slower than interp on this shape. | Tail-call/loop lowering in one frame, direct-threaded dispatch, fewer memo-boundary checks for local hot loops. | Needed to turn the fresh-temporary array-op floor into an LR-loop floor. Without it, stack reuse alone is not the Snark answer. |
| Dense table lookup missing | Exact requested dense 2D table cannot be expressed: no array get/index. | Add array indexing / `ARRAY_GET`, then bounds-check elision for proven table dimensions. | Not measured here; current results exclude table-index cost and therefore are not an optimistic dense-table result. |

## Verdict

With today's machine, the measured LR loop is about `232k x` off Rust in the
best isolated interpreter run at 10k tokens, and the JIT lane is about `1.30M x`
off. A 100k-token vix interpreter probe did not complete within 120s even in the
forced-copy control mode. That is not acceptable for lowering Snark's parser hot
kernels into vix today.

The headline is not "dispatch alone is too slow". The dominant current problem
is copy-path amplification from named molten reads: reuse exists, but this LR
idiom misses it. The fresh-temporary control proves that when reuse fires, the
interpreter can do array stack ops at ~77.5 ns/op, still ~67x a Rust `Vec` op
but orders of magnitude below the recursive LR loop result.

If consuming moves land for named rebinds, and if vix grows loop/tail-call
lowering that keeps the LR loop in one frame, the plausible ceiling moves from
hundreds-of-thousands-times Rust to the low-hundreds-times range for this tiny
automaton. Direct-threaded dispatch, projection-candidate bypass, array get, and
bounds-check elision are then the next gates. Until those are measured on the
same LR shape, Snark parse kernels should stay behind Rust/FFI rather than being
lowered wholesale into vix.

## Rematch after `0b62d0ec1`

Rebased this branch onto `origin/rodin` at:

```text
0b62d0ec1 vix: consume rebound molten update receivers
```

The focused benchmark tests still pass after the rebase:

```text
cargo nextest list -p vix -E 'binary(lr_loop_bench)'
cargo nextest run -p vix -E 'binary(lr_loop_bench)'
```

Result: the same 2 benchmark tests passed.

Important correction to the original attribution: the committed LR benchmark
does not actually use the fixed shadowing idiom. Its generated vix source uses
helper-call arguments such as `parse(next.1, stack.push(action), ...)` and
`parse(tokens, base.push(next_state), ...)`. The consuming-move predicate added
by `0b62d0ec1` applies to `let x = x.push(v)` and `let x = x.pop().1`, so this
unchanged LR artifact is a rematch of the old non-shadowing shape, not a proof
of the named-rebind ceiling.

### Unchanged LR Suite

Release build:

```text
cargo build --release -p vix --bin lr_loop_bench
```

10k tokens / 15k LR actions:

| lane | before, ns/action | after, ns/action | factor vs current Rust | note |
|---|---:|---:|---:|---|
| Rust | 2.898 | 1.040 | 1.0x | current rerun is faster; use after factor for current base |
| vix interp, `--molten-reuse` | 672,316.678 | 679,732.614 | 653,589x | effectively unchanged for this non-shadowing source |
| vix JIT, `--molten-reuse` | 3,779,842.308 | 1,753,279.183 | 1,685,845x | improved, but still much slower than interp |

If normalized against the original Rust baseline of 2.898 ns/action, the after
interpreter factor is `234,552x`, essentially the same headline as the original
`231,993x`. The apparent current-base factor is larger because the native Rust
baseline reran at ~1.04 ns/action.

100k-token probe:

| run | result |
|---|---|
| Rust, 100k tokens / 150k actions, 1000 runs | 1.031 ns/action |
| vix interp, 100k tokens / 150k actions, `--molten-reuse` | did not finish within a 120s alarm |

That remains a lower bound of `>800,000 ns/action`, or `>775,946x` against the
current 100k-token Rust baseline.

### Reuse Control After Rebase

The fresh-temporary control remains in the same range:

```text
./target/release/lr_loop_bench --mode array-control \
  --array-pushes 1024 --array-burst 32 --array-pops 16 --array-runs 100
```

| lane | before, ns/op | after, ns/op |
|---|---:|---:|
| Rust `Vec` | 1.149 | 1.309 |
| vix interp, forced copy | 204.621 | 191.429 |
| vix interp, reuse enabled | 77.520 | 73.383 |
| vix JIT, forced copy | 432.585 | 394.716 |
| vix JIT, reuse enabled | 284.445 | 265.830 |

That confirms the existing fresh-temporary reuse path still works and was not
the missing piece for the unchanged LR benchmark. The folded fix should be
measured with a new LR source that deliberately shadows `stack`/`tokens` as
`let stack = stack.push(...)` and `let stack = stack.pop().1`; the committed
source here is not that shape.

### stax Rematch

Profiled command:

```text
stax record -- ./target/release/lr_loop_bench \
  --terms 5000 --runs 1 --mode vix-interp --molten-reuse
stax flame -d 20 --threshold-pct 0.5 --run 7
stax top -n 25 --sort self --run 7
stax threads -n 20 --run 7
```

The profiled run printed `vix_interp_ns_per_action=719143.992`; stax reported
1.450s total active CPU and 130.021s off-CPU in run 7. Active trunk:

| stack / frame | active share |
|---|---:|
| `Machine::demand_i64 -> Driver::demand` | 98.3% |
| `weavy::task::Task::run_hosted` | 84.5% |
| `Driver::burst::{closure}` | 84.5% |
| `intern_molten_word` | 84.4% |
| `ValueStore::alloc_array_words` under `intern_molten_word` | 63.4% |
| `sha2::sha256::compress256` under allocation/canonical hashing | dominant leaf |
| generic iterator fold under `Driver::demand` | 11.9% |
| vix source parse/load | 1.6% |

So the new decomposition for the unchanged benchmark is still allocation and
canonical hashing under `intern_molten_word`, not host-call dispatch. The earlier
visible `projection_memo_hit -> projection_candidate_key` trunk is no longer a
large visible sibling in this rematch profile; it has been replaced by a generic
iterator-fold slice under `Driver::demand`, while SHA-256 remains the hot leaf.

### JIT Anomaly

The JIT-slower-than-interp anomaly persists:

| run | ns/action |
|---|---:|
| vix interp, 10k tokens | 679,732.614 |
| vix JIT, 10k tokens | 1,753,279.183 |

JIT is `2.58x` slower than interpreter on the rebased tree for this unchanged
recursive/memo-heavy LR source. That is less severe than the original `5.6x`,
but still a real finding: this workload is dominated by host/store/canonical
hashing paths, and JITting the weavy task lane does not remove that cost.
