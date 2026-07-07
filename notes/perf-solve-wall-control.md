# perf-solve-wall control findings

This note records the control measurements from the `perf-solve-wall` lane before
the standing perf lane took over follow-up work.

## Build profile control

All earlier ring wall-clock numbers in the mission thread were from nextest debug
test binaries. Re-running the same ring tests with the captured metadata reused
showed that the apparent solve wall mostly collapses in the profiling profile.

The control runs used:

```sh
TIER_A_CARGO_METADATA=/tmp/tier-a-scale-measurement/metadata.json
```

| ring | debug binary wall | profiling binary wall |
| --- | ---: | ---: |
| 32 | 6.06s | 1.20s |
| 64 | 20.30s | 2.99s |

The workspace profile has dependency-only dev optimizations for crates such as
`sha2`, `blake3`, `hashbrown`, and regex crates, but not local `vix` or `weavy`.
Use the profiling profile, not nextest's debug profile, as the baseline for
remaining solve-performance work.

## Weavy stencil control

Weavy stencils are already compiled with optimizations even when the host test
binary is a debug build. `weavy/build.rs` routes hostcall, async, and task
stencils through `copypatch::extract::compile_object`; that helper passes
`rustc --crate-type=lib -O -C panic=abort -C relocation-model=static`.

No stencil opt-level override was added.

## Metadata reuse

Commit `e120d009f Reuse captured metadata in tier-A harness` exports
`TIER_A_CARGO_METADATA="$OUT/metadata.json"` from
`scripts/tier-a-scale-measurement.sh`, so harness-side measurements reuse the
metadata JSON already captured by the script instead of re-running metadata
discovery inside each test.

## Corrected profiling baseline

Profiling-profile stax ring 64 was captured as run 30. The timing artifact
reported:

| step | time |
| --- | ---: |
| metadata | 25.741ms |
| machine load | 765.186ms |
| tree | 2.921ms |
| solve demand | 2363.154ms |
| lock/diff/render remainder | under 4ms combined |

The CPU profile was dominated by `Machine::demand_i64` / `Driver::demand`;
module loading was about 7% of sampled active time in this corrected baseline.

Ring 145 completed in the profiling profile:

| field | value |
| --- | ---: |
| wall | 23.10s |
| solve rows | 146 |
| lock rows | 893 |
| matches | 145 |
| version skew names | 0 |
| solve demand | 22228.877ms |

The old 180s timeout was therefore a debug-profile artifact for this lane, not
evidence of a remaining 180s off-CPU wait under the corrected profile.
