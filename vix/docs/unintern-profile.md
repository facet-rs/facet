# CDCL flesh benchmark after un-interning

This note records a profile-only pass over `vix/src/bin/cdcl_flesh_bench.rs`
after the CDCL hot loop stopped interning every iteration.

Commands:

```sh
cargo build --profile profiling -p vix --bin cdcl_flesh_bench
# temporary local edit only: RUNS = 3000, reverted after profiling
./target/profiling/cdcl_flesh_bench
stax record -F 50000 ./target/profiling/cdcl_flesh_bench
stax flame -d 18 --threshold-pct 1
stax top -n 25 --sort self
```

The selected stax run was `run 13`:

```text
run 13  [stopped]  pid 48968  84 kperf / 4 intervals  (./target/profiling/cdcl_flesh_bench)
```

The 3000-run non-profiled benchmark output was:

```text
naive_copy_ns=90578625
reuse_ns=48269417
rust_vec_ns=934958
```

The profiled run printed near-identical timings:

```text
naive_copy_ns=90661750
reuse_ns=48198125
rust_vec_ns=905208
```

Normalized from the non-profiled run, that is about 30.2 us per naive Vix demand,
16.1 us per reuse Vix demand, and 0.312 us per Rust Vec run.

## Top self-time frames

`stax flame` reported `total active 0.044s`, with 95.5% under
`vix::machine::lower::Machine::demand_i64`, 84.0% under
`weavy::task::Task::run_hosted`, and 72.7% under
`vix::machine::driver::Driver::burst::{{closure}}`.

Percentages below are computed against the flame total active time, 43.58 ms.
The function names and sample counts are from `stax top -n 25 --sort self`.

| self % | active ms | samples | frame |
| ---: | ---: | ---: | --- |
| 11.4 | 4.972 | 5 | `_xzm_free_main` |
| 11.3 | 4.935 | 5 | `weavy::task::Task::run_hosted` |
| 9.2 | 4.029 | 4 | `_platform_memmove` |
| 9.0 | 3.918 | 4 | `_xzm_xzone_malloc_tiny` |
| 9.0 | 3.918 | 4 | `vix::machine::driver::Driver::burst::{{closure}}` |
| 6.8 | 2.976 | 3 | `_malloc_zone_malloc` |
| 6.7 | 2.939 | 3 | `<core::hash::sip::Hasher<S> as core::hash::Hasher>::write` |
| 4.6 | 1.996 | 2 | `__rustc::__rdl_alloc` |

The next rows were `core::slice::copy_from_slice_impl::len_mismatch_fail`
at 4.6%, `core::hash::BuildHasher::hash_one` at 4.5%,
`_xzm_malloc_pac` at 4.5%, and one-sample rows for `_free`, `_xzm_free`,
`alloc::raw_vec::RawVecInner<A>::reserve::do_reserve_and_handle`,
`vix::machine::driver::intern_flesh_word`, `blake3::portable::compress_in_place`,
`snark::lower::weavy::RuntimeWeavyStepper::runtime_reduce_fragment`,
`vix::machine::driver::write_variant_tag`, and `xzm_malloc_zone_size`.

No `sha2`/`Sha256` frame appeared in `stax top -n 200 --sort self` or in a
threshold-zero flame. The observed SHA-256 self-time in this run is therefore
0.0%, with this run's one-sample resolution around 2.2% of active time. The only
sampled cryptographic hash-library frame was load-time `blake3::portable::compress_in_place`
at 0.98 ms / 2.2%, under `phon_schema::identity::resolve_ids`.

## Classification

The current measured hot path is not SHA-256. It is the flesh execution path
inside `Driver::burst`.

Allocator traffic is the largest visible class. Direct allocator and allocator
wrapper self-frames account for roughly 40-45% of observed active time:
`_xzm_free_main`, `_xzm_xzone_malloc_tiny`, `_malloc_zone_malloc`,
`__rustc::__rdl_alloc`, `_xzm_malloc_pac`, `_free`, `_xzm_free`, and
`xzm_malloc_zone_size`.

Copy width is also visible, but smaller than allocator traffic in this run:
`_platform_memmove` is 9.2% self, and the `FleshStore::array_entry` subtree is
13.8% total. That subtree clones both `elem_schema` and `words` when returning
`ArrayEntry::Words` from `vix/src/machine/driver.rs`.

HashMap/hash work is present but not dominant. SipHash hasher self-time is 6.7%,
`BuildHasher::hash_one` is 4.5%, and the one visible `HashMap::insert` subtree is
2.2% in the flame.

Weavy interpreter dispatch is visible as `weavy::task::Task::run_hosted` at
11.3% self. The bulk of its total time descends into `Driver::burst`, so the
profile does not point at dispatch alone as the next lever.

Vec growth/reallocation is visible but secondary: `RawVec::grow_one` is 6.9%
total in the flame and reaches `_realloc`, which itself splits into allocator
work and a 2.3% `_platform_memmove` child.

Refcount inc/dec does not show up as a measured top-frame class. The relevant
`flesh_dup` path was not sampled as a hot leaf in this run.

BTreeMap work is not a hot-loop class here. The only BTreeMap frame in the
threshold-zero flame is load-time module-table construction at 2.2%.

## Arena allocator verdict

The data supports an arena/bump allocator experiment for flesh-tier frame-local
values. The measured dominant class is allocator traffic, not SHA-256, not map
ops, and not refcounting.

A per-frame arena that keeps hot-loop flesh tuples/arrays out of system malloc
could plausibly remove the allocator-class self-time, about 40-45% of active
time in this short run. It would not remove the remaining measured costs:
array word/schema copying, `memmove`, SipHash/hash-one, `Driver::burst` body
work, and `Task::run_hosted` dispatch. The follow-up lever after allocator
traffic is the copy/materialization path around `FleshStore::array_entry`,
`array_push`, and `array_pop`, but that is a second lever in this profile, not
the measured dominant class.
