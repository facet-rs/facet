# The foundation contract

The rungs say what the language does. This file says what an
implementation must BE, so that fifty green rungs can never be a
tree-walking interpreter with fifty caches bolted on. It exists because
that exact failure already happened once — the previous engine grew 64
ad-hoc host calls and a dozen private caches, and its autopsy became the
machine spec. These constraints are how the ladder refuses to let it
happen twice.

**This contract is as immutable as the rungs. An implementing agent who
finds it impossible stops and reports; it does not reinterpret.**

## 1. The runner contract applies from rung 001

`vx test` does not merely report pass/fail. For EVERY test, from the
first rung, the runner must produce:

- a **demand trace** (the sequence of demand/memo/effect events) — the
  `expecting` clauses of later rungs are assertions over this data, and
  the data must exist and be well-formed even where no rung asserts on
  it yet. No trace, no score — a runner that cannot say what was
  demanded is not running vix.
- **standing counters**: memo hits/misses, allocations on the memo hit
  path (must be zero — `vix.memo.allocation-free-hits`), host calls per
  pure operation (must be zero once combinators land —
  `vixc.execution.no-pure-hostcalls`), store interns, effect spawns.
- a **receipt** (recorded read-set) for every demand that touched a
  store-backed or external value, complete by construction
  (`vix.receipt.witness-reads` — reads go through recording accessors;
  there is no non-recording read path to forget).

## 2. Chaos is on from day one

`vx test --ratchet --chaos` randomly kills in-flight work and asserts
results identical to the plain run (`vix.scheduler.chaos-kill-oracle`,
SETTLED day one; replay is the semantics, suspension is an acceleration).
The ratchet score is only valid if the chaos run agrees. An architecture
that cannot be killed anywhere cannot pass rung 001 — which is the point:
restart-safety cannot be retrofitted, so it is priced into the first
green.

## 3. Score bands are gated on spec coverage

The rules live in the vix book's Specification section and carry
`r[impl]` / `r[verify]` annotations checked by `ddc coverage`. Scoring
past a band boundary requires the band's rule set to show implementation
coverage — behavior alone does not advance the ratchet:

| To score past | Coverage required (rule families) |
|---|---|
| 001 (any score at all) | store handles + entry-carried identity (`identity.value-identity-pair`, `identity.hash-at-construction`, `store.handle-opaque`, `store.dedup`), the demand map + memo key (`memo.demand-key`, `memo.no-recompute-at-lookup`), the event vocabulary (`obs.event-vocabulary`, `obs.event-sink`) |
| 025 | the checker surface the reject rungs exercise; typed errors (no stringly `Result<_, String>`) |
| 052 | scheduler (`replay-is-semantics`, `join-atomic`, `live-budget`, spawn-and-park), three-tier memo EXACT tier, `cache.three-kinds` + `no-private-caches` (the counters must account for every reuse event against exactly three caches) |
| 066 | typed decode through the doc-parse primitive — one host call per document, no Doc-walking on hot paths |
| 082 | the primitive trait + `EffectCtx` witness discipline, receipts (`complete-by-construction`, `misses-recorded`), journal, persistence (`value-vs-claim`, `reverify-on-load`) — rungs 079–081 are unpassable without a real persistent store and PROJECTION-tier verification, by design |
| 100 | the remaining SETTLED rules of the runtime spec; open DESIGN rules explicitly excepted |

## 4. Structural bans, checked mechanically

- **No private caches, ever** (`vix.cache.no-private-caches`): every
  reusable result is a store value or memo entry. The counters in §1
  make a hidden cache visible as unaccounted reuse.
- **No pure host calls** once the relevant band lands: map/array/option/
  string/compare are lowered vocabulary, not FFI.
- **One hash definition** — the schema-specialized framed walk; the
  content hash is computed at intern and read as a field thereafter.
  Anything hashing raw memory or maintaining a second digest scheme is a
  fork, and forks are the disease this spec exists to cure.
- **Semantic order is content order.** Hash bytes are never
  ordering-visible. (Rung 035 checks the behavior; this line bans the
  implementation that passes it accidentally.)
- **The scheduler is passive data**; weavy owns suspension; no shadow
  scheduler, no clock, no poll loops (`block-on-event`).

## 5. The performance floor: budgets with asymptotic gaps

Traces and coverage stop the wrong architecture; budgets stop the *slow
faithful* one — the interpreter-on-weavy that emits valid traces at ten
microseconds per operation and climbs the ladder legally. Selected rungs
carry `//! budget: <wall>, <rss>` headers (the runner kills over-budget
runs; a killed rung is red) and counter expectations:

```
scheduler_requests_at_most N   — machinery contacts during the test
memo_entries_at_most N         — no per-iteration memoization
store_interns_at_most N        — publish-once: molten until the end
```

The budgets are honest because the gaps are asymptotic, never
constant-factor — robust to any machine, unreachable by brute force:

- **Rung 050** (10M-iteration tail loop, 5s): free for a fused interior;
  100+ seconds for anything touching scheduler/memo/identity machinery
  per iteration. Forces interior iteration to be *not demand*.
- **Rung 051** (1M-element array accumulator, 5s): the molten rung.
  Without uniqueness-mutation and publish-once, each push either copies
  (O(n²) — ~10¹² word moves, weeks) or interns (forbidden by the
  counter). There is no third way; that is the rung's entire content.
- **Rungs 098/100** (the solver, 30s): keeps the capstone honest once
  everything composes.

Memory budgets are enforced the same way (the previous engine once
allocated 22GB in four seconds on a 100k-iteration accumulator; the
budget line exists because that happened).

## 6. What the bands buy the climber

The gates are not bureaucracy; they are the re-architecture Amos is
worried about, priced at the moment it is cheapest. The trace/chaos/
identity obligations at rung 001 cost a day when the system is a hundred
lines and a rewrite when it is ten thousand. An agent who reaches rung
050 on this foundation reaches rung 100 on it too — that is the claim,
and the autopsy of the previous engine is the evidence for it.
