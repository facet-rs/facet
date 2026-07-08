+++
title = "machine: memo"
+++

Memoization: keys, hit economics, and the three-tier verified-reuse ladder
preserved from the current driver (the part of the old machine that was
right).

r[machine.memo.demand-key]

[DESIGN] The memo is keyed by `DemandKey`: a fixed-size digest formed by a
framed combine of the closure identity and argument identities (each argument
identity is the pair `(SchemaRef, ContentHash)`, per
`machine.identity.value-identity-pair`), computed once at demand entry. The
heap-allocated tuple key and its per-lookup element hashing are banned. To
prevent a digest collision serving a wrong value, each memo entry also
carries the exact key preimage `(closure identity, arity,
[(SchemaRef, ContentHash)])`, and exact lookup compares the preimage after
the digest map hit. An implementation that instead relies on blake3
collision-resistance must state that as its trust assumption and reject
untrusted persistent exact claims unless policy permits it.

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
(`machine.receipt.witness-reads`), not conventional. The reads the machine
performs TO re-verify a candidate are machine-meta operations and are not
themselves recorded into any receipt (they are not the demanded
computation's reads).

r[machine.memo.receipt-remap]

[DESIGN] A memo hit contributes the cached entry's read-set to the CALLER's
receipt (the caller depended on everything the cached computation read). This
remapping is what keeps receipts complete across nested reuse. Exact hits
stay allocation-free (`machine.memo.allocation-free-hits`) by pre-materializing
the exposable read-set value at miss time, so the hit path only references
it. (Preserved from the driver's nested-hit read-set remapping.)

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
