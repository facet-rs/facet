+++
title = "machine: observability"
+++

The observability spine — built first, zero-cost when off, and the reason
"why is it slow" and "why did this rebuild" are queries rather than
campaigns.

r[machine.obs.event-sink]

[SETTLED] Events flow through a sink trait parameter. A no-op sink inlines
to nothing — observability is free when off, always available when wanted.
Unconditional emission on hot paths is banned.

r[machine.obs.event-vocabulary]

[DESIGN] The demand-event vocabulary preserves the current driver's
distinctions: demanded, the three memo-hit tiers with verified counts,
spawned (fn-level and exact-invocation-level separately), parked, completed,
store-alloc with dedup bit, the exec lifecycle triple carrying the tier
verdict, generic observations with replay bit, and artifact-probe events
with their own cache bit. Four reuse axes, four event shapes
(`machine.arch.reuse-axes-distinct`).

r[machine.obs.counters]

[DESIGN] Standing counters: hostcalls per inner-loop iteration, scheduler
requests per iteration, interns per branch, memo hit economics (hits,
verified-counts, allocation on hit = 0), effect-pool saturation. Counters
are load-independent attribution — the instrument of record when wall
clocks lie.

r[machine.obs.stax]

[DESIGN] Stax sees everything: jitdump/DWARF registration wired for every
compiled stencil (the mechanism exists in weavy; WIRING it is the rule),
phase spans at machine boundaries (demand entry, freeze, effect begin/
complete) as user events. A profile with anonymous regions is a spec
violation, not a tooling gap.

r[machine.obs.ir-dump]

[DESIGN] Lowered weavy IR has a first-class text dump. A machine whose
compiled form cannot be read is unreviewable by construction.

r[machine.obs.snapshot-no-clone]

[SETTLED] Observability surfaces do not clone the world: a debug/inspection
snapshot is a view, iterator, or copy-on-write, never an O(store) allocation.
(L11's observability half; the store half is `machine.store.snapshot-no-clone`.)

r[machine.obs.loud-fallbacks]

[SETTLED] Every conservative fallback — safe-miss, safe-rebuild, safe-false
— emits a reasoned event. A receipts system cannot contain a rebuild cause
that leaves no receipt; "correct but slow" gets the same rigor as "wrong"
because nothing else ever catches it.

r[machine.obs.flakiness-detected]

[DESIGN] Nondeterminism is machine-detected: diverging read-sets or results
on a memo re-run (`machine.memo.rerun-audit`) surface as typed events.
Flakiness is an observation, not folklore.
