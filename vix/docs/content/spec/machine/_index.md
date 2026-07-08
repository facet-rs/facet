+++
title = "machine: the vix runtime"
+++

Normative specification for the vix machine — the demand scheduler,
content-addressed value store, memoization, receipts, host primitives, and
observability spine underneath the vix language and the vixen build system.
This spec governs the runtime rewrite: implementations carry `r[impl id]`
references from their first commit, tests carry `r[verify id]`, and
`ddc coverage` is the fidelity authority.

Provenance: the 30-law audit constitution and battle plan (2026-07-08), the
hostcall census, `rodin/docs/90-substrate-ledger.md`, the hash-as-field
proposal (committee-converged), the vixen KB design corpus, and an
archaeology sweep of the current driver ("the accidental spec") whose
correct semantics are preserved here and whose defects are explicitly not.

Every rule carries a confidence marker:

- **[SETTLED]** — decreed or committee-converged; changing it needs Amos.
- **[DESIGN]** — concrete agreed design; changing it needs a written reason.
- **[OPEN]** — a decision is required before or during implementation; the
  rule states the alternatives rather than picking one.

## Charter rules

> r[machine.cache.three-kinds]
>
> [SETTLED] The machine has exactly three caches: the **store** (interned
> canonical values, keyed by content identity), the **memo** (demand results,
> keyed by `DemandKey`), and **lowering artifacts** (keyed by program
> identity). No other cache may exist.

> r[machine.cache.no-private-caches]
>
> [SETTLED] No primitive, capability, or subsystem may hold a private result
> cache. Every reusable result is a store value or a memo entry. (The old
> machine's `oci_file_memo`, ELF projection memos, and fetch journals are the
> named counter-examples.)

> r[machine.cache.effects-are-memo-entries]
>
> [SETTLED] Effect results enter the memo as ordinary entries keyed by their
> demand. An effect with a private table is a spec violation regardless of its
> hit rate.

> r[machine.arch.scheduler-is-passive]
>
> [SETTLED] The scheduler is passive data behind the same primitive interface
> as everything else. There is no central loop, no clock, and no
> channels-as-architecture. Work is driven by executing tasks calling
> scheduler primitives and by completions resuming parked tasks.

> r[machine.arch.one-authority]
>
> [SETTLED] Every question has exactly one owning authority: weavy owns
> execution mode and suspension; the scheduler owns admission; the store owns
> handles; the identity module owns hashing. A consumed abstraction is never
> re-implemented above its owner.

> r[machine.arch.observability-first]
>
> [DESIGN] The observability spine (event sink, counters, stax/jitdump wiring,
> IR dump) exists before the subsystems it observes. No perf-relevant
> subsystem lands without its counter.

> r[machine.arch.reuse-axes-distinct]
>
> [DESIGN] The machine has four distinct reuse EVENT FAMILIES with separate
> verification stories: pure-function memo (exact/projection/semantic), store
> dedup, exec tier-1/tier-2, and artifact-probe reuse. These are four event
> shapes, not four caches: there are still exactly three CACHE KINDS
> (`machine.cache.three-kinds`). Artifact probes are memoized primitive calls —
> their key is (primitive id, request value), their receipt is the primitive
> receipt, their storage is the memo. The distinction the rule preserves is
> that the four families are never collapsed into one generic "cache hit"
> event.
