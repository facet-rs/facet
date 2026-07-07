# JIT lane slower than interp — investigation report

Branch/worktree: `jit-lane-investigation` at
`/Users/amos/.paseo/worktrees/1t3lgrd0/jit-lane-investigation`, base `0f4c2a40f`
(post V3 hash-epoch stages 1-4, post tail-loop lowering, post JIT compile cache —
everything NEXT.md's investigation note said was already folded).

## Headline

The 2x JIT-vs-interp anomaly from spike D's final measurement (`fc0eff22d`,
1.65x at 10k tokens / 2.03x at 100k tokens) **is mostly gone** on current trunk.
Re-measured on `0f4c2a40f`: JIT is now only **~1.06x-1.18x** slower than interp,
which is within the noise band of a heavily-loaded shared machine (other agents
building throughout, load average 33-51 during these runs). The remaining gap is
real but small, and it has a concrete, identified cause: a shared Rust host-call
cost (string/hash-keyed schema lookups for array/container element types) that
both lanes pay, but which consumes a much larger *fraction* of the JIT lane's
active time because JIT already removed its own interpreter-dispatch overhead
(Amdahl's-law exposure), plus what looks like a real per-call JIT→host transition
tax on top.

Mid-investigation, `origin/rodin` advanced past this branch's base and folded
**exactly the fix this investigation was converging on** (containers-as-declared-
descriptors, commits `ef60d46c8..b18870fc0`, plus `1e75f74b6`) — this happened
live, from another agent, while this report was being written. A same-day
re-check on that new tip (scratch worktree, not merged into this branch) shows
the array-op JIT/interp ratio dropping further, to ~1.1x-1.2x, though the
underlying string-lookup call sites are still present in the code (see below) —
so the improvement is not fully explained yet, and the full recursive LR
benchmark can no longer be measured on the newest tip without further, unrelated
work (a fresh language gap the fold surfaced).

## Method

Reused spike D's benchmark harness (`vix/src/bin/lr_loop_bench.rs`, LR grammar
`E -> ID | E PLUS ID`, tail-recursive `parse(...)` with a molten `Array<Int>`
parse stack), copied from `lr-loop-vix-baseline` into this branch since it never
folded onto `origin/rodin` and the currently-assigned base predates it. One
fixup was required to build:

```
fn parse(remaining: Int, stack: Array, state: Int, lookahead: Int, reduces: Int) -> Int {
```

still builds and runs fine on `0f4c2a40f` (bare `Array` accepted).

Measurements below are release-build binaries (`cargo build --release -p vix
--bin lr_loop_bench --features jit`), independent single-demand process runs (as
spike D found: reusing one loaded machine across demands is not representative —
driver state grows). Medians of ~14 runs each token size to damp shared-machine
noise; `stax` (launch-mode, full-run capture with `debug = "line-tables-only"`
via the workspace `profiling` profile) for cost attribution.

## Measurements (base `0f4c2a40f`)

| tokens | lane | median ns/action (n=14) | range | ratio JIT/interp |
|---|---|---:|---:|---:|
| 10k (5000 terms, 15k actions) | interp | 791.4 | 681-915 | — |
| 10k | JIT | 838.9 | 739-972 | **1.06x** |
| 100k (50000 terms, 150k actions) | interp | 693.3 | 570-840 | — |
| 100k | JIT | 819.6 | 655-998 | **1.18x** |

Rust reference: ~1.0-1.06 ns/action at 100k tokens (1000 runs) — noisy at this
scale (spike D's own report saw 1.03-2.9 ns/action across reruns), so it is not
used here as anything but a sanity check; the interp/JIT ratio is the load-bearing
number since it is internally consistent (same process, same run, immune to
Rust-baseline jitter).

Compare to spike D final (`REPORT.md`, same grammar, `fc0eff22d`, i.e. tail
loops + JIT compile cache already folded, but *before* V3 hash-epoch stages 1-4):

| tokens | interp median ns/action | JIT median ns/action | ratio |
|---|---:|---:|---:|
| 10k | 1828.9 | 3022.9 | 1.65x |
| 100k | 863.5 | 1750.3 | 2.03x |

Both lanes got substantially faster in absolute terms (interp ~2.3-2.6x, JIT
~3.6-3.9x) and the ratio between them collapsed from 1.65x-2.03x to ~1.06x-1.18x.
Given the run-to-run spread at each size overlaps by more than that residual
gap, **the 2x finding does not reproduce on current trunk** — it shrank, as
NEXT.md's investigation note anticipated.

## stax evidence

Full-run captures (`stax record -F 900 -- ./target/profiling/lr_loop_bench
--tokens 10000000 --runs 1 --mode <lane> --molten-reuse`), `debug =
"line-tables-only"` via the workspace `profiling` profile.

### Interp (run 20, 4809 kperf samples, 2.168s total active)

```
Driver::burst::{{closure}}                              67.2%
  SchemaTables::display_ref                              23.0%  -> memmove 21.3%
  _xzm_malloc_pac (allocator)                             21.7%
  SchemaTables::kind_for_name -> legacy_ref                4.3%  -> hash_one 1.1%, memcmp 0.3%
  DescriptorMap::get                                       2.7%  -> hash_one 1.7%
  String::clone                                            2.1%
  MoltenStore::array_entry                                 1.7%
  RawVec::grow_one (realloc)                               1.4%
  SchemaTables::name_for_frame_word                        0.7%  -> hash_one 0.7%
```

### JIT (run 21, 736 kperf samples, 1.744s total active)

```
JitTask::run_hosted (92.0% of process)
  Driver::burst::{{closure}}                              91.0%
    SchemaTables::name_for_frame_word                      30.8%  -> hash_one 30.7% -> siphash write 15.5%
    _xzm_xzone_malloc_tiny (allocator)                     26.5%
    Vec::from_iter                                         15.7%  -> malloc_pac 15.5%
    String::clone                                          15.5%  -> malloc_freelist_outlined 15.4%
    SchemaTables::kind_for_name -> legacy_ref                0.6%
    MoltenStore::array_entry                                0.5%
    DescriptorMap::get                                      0.4%
  Machine::load_with_lane (parse/lower, one-time)          7.8%
```

The striking number: `SchemaTables::name_for_frame_word` — a single hash-map
lookup keyed by `SchemaId` (`u64`) — is 0.7% of interp's active time but **30.8%**
of JIT's, on the exact same shared Rust function (`vix/src/module.rs:181-187`,
called from `vix/src/machine/driver.rs:9575-9580`'s `schema_name_for`). That
`frame_names` map (`vix/src/module.rs:141-146`) is a plain
`std::collections::HashMap<SchemaId, String>` — default hasher, i.e. SipHash-1-3 —
hashing an already-uniformly-distributed 64-bit id. SipHash's multi-round mixing
on a key that needs none of it is pure waste, and it is the dominant leaf
(`<sip::Hasher as Hasher>::write`, 15.5% of JIT's *total* active time by itself).

### What's actually on this hot path, and why

Every array/map/handle host op resolves its element's schema through a
name↔id bridge that is still string-keyed at its core, even after V3 stages 1-4
(which addressed *host-call argument* dispatch, not *container element*
dispatch):

- `vix/src/module.rs:141-146` — `SchemaTables` keeps `by_name:
  HashMap<String, SchemaRef>`, `frame_names: HashMap<SchemaId, String>`.
- `vix/src/module.rs:200-212` — `legacy_ref(name: &str)` does the `by_name`
  string-hash lookup (or builds a fresh `SchemaRef::var` / recurses into
  generic-arg parsing for names like `Array<Int>` it doesn't recognize).
- `vix/src/module.rs:249-251` — `kind_for_name` = `kind_for_ref(&legacy_ref(name))`.
  `is_primitive`/`is_list`/`is_map`/`is_option`/`is_external`/`is_named_schema`
  (`:253-298`) all funnel through this, i.e. **every one of those predicate
  checks is a fresh string-hash lookup**, and `vix/src/machine/driver.rs:7469-7538`
  (`compare_words`, the value-comparison used by `match`) calls several of them
  in sequence for a single comparison.
- `vix/src/module.rs:221-240` — `display_ref` *allocates* a fresh `String`
  (recursively, `format!("{base}<{}>", …)` for generics) just to hand a `&str`
  back into the same string-keyed lookup machinery — a pure round-trip cost.
- `vix/src/machine/driver.rs:9571-9580` — `schema_ref_for`/`schema_name_for`,
  the marshaling pair that converts a schema name to/from the `i64` "frame word"
  carried in molten array/map entries. `schema_name_for` (→
  `name_for_frame_word`) is called from ~20 call sites across array push/pop/
  set/len, map get/set, option realize, and handle dispatch (`driver.rs:1407,
  1565, 2748, 2969-3002, 3126-3130, 3251, 3319, 3679, 3810, 3948, 4431, 4738,
  4774, 5023, 9510-9512`) — i.e. this is the generic per-op schema check for
  *every* container mutation, not an edge case.

This is the language-level gap NEXT.md's epoch status already named as
outstanding: "containers as declared descriptors" (V1) hadn't folded at this
branch's base — array/map elements still carry a **String** schema tag at the
`MoltenStore` layer, so every push/pop/compare re-derives `Kind` by hashing (and
sometimes allocating) that string, on both lanes.

## Hypotheses

1. **JIT→host trampoline cost / Amdahl's-law exposure of a shared host-call
   cost — CONFIRMED (primary driver).** The array/container schema-lookup code
   above runs as ordinary Rust in *both* lanes (`Driver::burst::{{closure}}` is
   the same closure regardless of lane — interp calls it from
   `weavy::task::Task::run_hosted`, JIT from
   `weavy::jit::task_lane::JitTask::run_hosted`). JIT's compiled code removes
   its own dispatch/decode overhead for the parts it *can* compile (arithmetic,
   branches, the tail-loop backward jump), which is exactly why JIT's *total*
   active time is smaller than interp's for the same workload. But every host
   op still has to leave JIT-compiled machine code and re-enter this same Rust
   function — and that host-call cost, which barely dents interp's larger
   total, now dominates JIT's smaller remaining budget. This explains both the
   percentage-share inversion (0.7% -> 30.8% for the same function) and gives a
   mechanism for a genuine added JIT-side cost on top of the share inversion
   (the actual code-to-native transition), though isolating that transition's
   fixed per-call cost from the pure Amdahl's-law reweighting would need a
   micro-benchmark that varies host-call density independent of loop-body size
   — not done here, flagged as a follow-up if the residual gap doesn't close on
   its own once the string lookups are cheaper.
2. **Frame/register spill shape forcing extra memmove — REJECTED as the
   driver.** `_platform_memmove` does appear in both flames, but exclusively as
   a child of *string* operations (`display_ref`'s formatting, `String::clone`,
   `RawVec::grow_one` reallocation) — not as a sibling of JIT-specific
   register/stack-frame management. No JIT-only memmove trunk was found.
3. **Cache effects from jumping between JIT code pages and host code —
   REJECTED / unsupported.** kperf CPU-cycle sampling doesn't directly measure
   cache misses, but if code-page-switching were dominant we'd expect it to
   show up as unattributed/unresolved time near the JIT↔host boundary; run 21
   has no such trunk above noise (a handful of `<unresolved>` samples, <1%).
   Can't fully rule out a minor contribution without PMU cache-miss counters,
   but it is not the confirmed cost here.
4. **Burst-boundary bookkeeping done per-call in JIT vs batched in interp —
   REJECTED.** `Driver::burst::{{closure}}` is one Rust closure shared by both
   lanes (confirmed by reading the call sites in `driver.rs`); there is no
   JIT-specific duplicate bookkeeping path — the bookkeeping is identical code,
   it's just a bigger fraction of a smaller total.

## The fix landed mid-investigation (and what it changed)

`origin/rodin` gained 6 commits past this branch's base while this report was
being written: `ef60d46c8` "declare typed container descriptors" through
`b18870fc0` "coerce doc arrays into typed arrays", plus `1e75f74b6` "Expand
workspace member globs in vix" — the `hash-epoch-containers` mission NEXT.md
listed as still in flight. That is precisely the "containers as declared
descriptors" direction this investigation's evidence points at as the fix.

To check its effect without touching this branch's history, I built a scratch
worktree at `origin/rodin`'s new tip (`git worktree add --detach
/tmp/rodin-tip-check origin/rodin`, removed after use) and re-ran the
non-recursive array-op control benchmark (`--mode array-control --array-pushes
1024 --array-burst 32 --array-pops 16 --array-runs 200`, same code as spike D's
"fresh-temporary reuse control"):

| lane / mode | old report (pre-V3, `lr-loop-vix-baseline` rebase) | new tip (`1e75f74b6`) |
|---|---:|---:|
| Rust | 1.309 ns/op | 0.972 ns/op |
| interp, forced copy | 191.429 ns/op | 286.028 ns/op |
| interp, reuse | 73.383 ns/op | 154.095 ns/op |
| JIT, forced copy | 394.716 ns/op | 298.852 ns/op |
| JIT, reuse | 265.830 ns/op | 173.397 ns/op |
| **JIT/interp ratio, reuse** | **3.62x** | **1.13x** |
| **JIT/interp ratio, copy** | **2.06x** | **1.04x** |

The JIT/interp ratio on pure array ops collapsed from ~3.6x to ~1.1x — a much
bigger and cleaner signal than the LR-loop numbers above, because this
benchmark isolates array push/pop/len from recursion/tail-loop bookkeeping.
Notably, JIT got *faster* in absolute terms (265.8 -> 173.4 ns/op) while interp
got *slower* (73.4 -> 154.1 ns/op) — so this is not simply "JIT caught up to an
unchanged interp floor"; something in the fold measurably shifted cost between
the lanes.

**However**, a `stax` capture on that same new-tip binary (force-copy path,
both lanes in one process, 1578 kperf samples — read directionally only, this
run mixes phases and is noisy under shared-machine contention) shows
`legacy_ref` / `kind_for_name` / `display_ref` / `name_for_frame_word` /
`DescriptorMap::get` **still present** in both lanes' hot array-push/pop path.
So the fold did not remove these call sites outright — the ratio improvement is
real (directly measured, not a stax artifact) but its exact mechanism is not
fully pinned down by this investigation. Worth a dedicated follow-up once more
of the epoch settles.

**Blocking gap found on the new tip**: the full recursive LR benchmark no
longer lowers cleanly. `stack: Array` (untyped) is now rejected ("Array is not
an Array<T>") — fixed locally by typing it `Array<Int>` — but that then hits
`"lowering parse: no schema ref for Tuple<Int,Array<Int>>"`, because `.pop()` on
a typed array returns `Tuple<Int, Array<Int>>` and that generic tuple shape has
no registered schema ref yet. This is a fresh language-level gap the fold
surfaced (consistent with NEXT.md's existing "Array.pop surfaces as
Tuple<Int,Array> (awkward for non-Int)" gap note), not something to fix as part
of this read-only investigation. It blocks a like-for-like LR-loop
re-measurement on the newest trunk until resolved.

## Proposed fix shape (not implemented)

1. **Finish and land what's now `origin/rodin`'s tip**: fold this branch's
   base forward past `1e75f74b6` once it's safe to do so, so future JIT-lane
   work starts from the container-descriptor world rather than the pre-fold one
   measured here.
2. **Resolve the `Tuple<Int, Array<T>>` schema-ref gap** surfaced by that fold
   (register a schema ref for the generic tuple shape `.pop()`/`.push()`
   produce, or special-case it in `descriptor_key_for_name`/`legacy_ref`) so the
   natural recursive LR benchmark can be re-measured end to end on the new tip.
3. **Swap `SchemaId`-keyed maps off the default SipHash hasher.**
   `frame_names: HashMap<SchemaId, String>` (`vix/src/module.rs:145`) and
   similarly-keyed maps hash an already-uniform 64-bit id with a
   multi-round DoS-resistant hasher for no benefit — `rustc_hash::FxHashMap` or
   an identity/pass-through hasher for `SchemaId` removes `sip::Hasher::write`
   (15.5% of JIT's *total* active time in run 21) essentially for free, no
   design change required.
4. **Once (1)-(3) land, re-run this same stax capture** to see whether the
   Amdahl's-law share-inversion in hypothesis 1 resolves on its own (cheaper
   shared host-call cost -> smaller absolute JIT tax) or whether a genuine
   per-call JIT→host transition cost remains and needs its own treatment
   (e.g., inlining simple push/pop directly into JIT-generated code instead of
   trampolining to the shared host closure, or batching several container ops
   per host-call round trip from JIT-compiled code).

## Verdict

The originally-reported "JIT ~2x slower than interp" does not reproduce on
current trunk (`0f4c2a40f`): it measures at ~1.06x-1.18x, inside this shared
machine's noise band. The residual gap has an identified, evidence-backed
mechanism — a shared string/SipHash-keyed schema-lookup cost on the
array/container host-op path that both lanes pay, but that dominates a larger
share of JIT's smaller post-dispatch-removal budget — and that mechanism is
exactly what the just-landed `hash-epoch-containers` fold targets. A same-day
check on that fold's tip shows the array-op-specific ratio improving further
(to ~1.1x-1.2x) but not disappearing, and the full LR-loop benchmark is
currently blocked from re-measurement on that tip by a fresh, unrelated typed-
tuple schema-ref gap. Recommended next step is fixing that gap and re-running
this same measurement/stax pair on the folded tip, not further speculative
optimization.
