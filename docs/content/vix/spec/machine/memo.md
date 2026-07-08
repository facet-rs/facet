+++
title = "machine: memo"
+++

Memoization: keys, hit economics, and the three-tier verified-reuse ladder
preserved from the current driver (the part of the old machine that was
right).

r[machine.memo.demand-key]

[DESIGN] The memo is keyed by `DemandKey`: a fixed-size digest formed by an
ordered, domain-separated combine of the closure identity and argument
identities (`machine.identity.streaming-combine` applies), computed once at
demand entry. The heap-allocated tuple key (`(u64, Vec<ContentHash>)`) and
its per-lookup element hashing are banned.

r[machine.memo.no-recompute-at-lookup]

[DESIGN] Memo lookup reads stored identity slots
(`machine.identity.hash-at-construction`). Canonicalizing or hashing
arguments at lookup time is banned — the memoizer must not pay identity
recomputation as its entry fee.

r[machine.memo.allocation-free-hits]

[SETTLED] A memo hit is allocation-free: borrowed or Arc'd returns.
`.cloned()` on the hit path has its economics backwards.

r[machine.memo.three-tier-reuse]

[DESIGN] Warm reuse tries three tiers in order — exact (key equality),
projection (a prior entry whose recorded read-set re-verifies against the
current arguments), semantic (declared per-argument comparator functions,
themselves demanded/memoized) — then spawns. Each tier emits its own event
with a `verified` count. (Preserved from `Driver::demand`.)

r[machine.memo.verified-reuse]

[SETTLED] Reuse is verified, never trusted-on-record: a projection hit
re-checks every recorded read against the current world before serving. An
entry with an empty read-set is never a projection candidate. Verification
against an under-recorded read-set is unsound in the dangerous direction —
it serves stale values — which is why read-set completeness is structural
(`machine.receipt.witness-reads`), not conventional.

r[machine.memo.hit-carries-receipt]

[DESIGN] A memo hit exposes the original entry's receipt (read-set), not
just the cached value. Consumers — test selection, audit mode, widening —
need "why is this reusable," not a bare boolean. (Testing-as-demand: an
unchanged test is a cache hit that carries its receipt, never a heuristic
skip.)

r[machine.memo.effect-results]

[SETTLED] Effect (primitive) results are memo entries keyed by their demand,
under the primitive's declared memo policy. Private effect caches are
banned (`machine.cache.no-private-caches`).

r[machine.memo.rerun-audit]

[DESIGN] The machine can re-run a memoized node and diff its result and
read-set against the recorded entry. This one capability serves audit mode
(statistical re-verification of memoized tests), flakiness detection
(diverging read-sets or results under identical inputs are machine-detected
facts, not folklore), and tier-2 style re-verification generally.
