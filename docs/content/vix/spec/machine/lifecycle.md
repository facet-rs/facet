+++
title = "machine: effect lifecycle & failure"
+++

What happens when things do not complete cleanly: effect failure, tickets
that never resolve, freeze that fails partway, poison arriving mid-flight,
and the world mutating during a demand. The happy path is specified across
the other pages; this page is the rest of the state space. Added after
adversarial review found the spec happy-path-complete but failure-thin.

## Effect failure

> r[machine.lifecycle.effect-failure-is-a-result]
>
> [DESIGN] A failed effect — exec exits non-zero, fetch 404s, a test fails —
> is a receipted RESULT, not a `MachineError`. It memoizes under the
> primitive's policy (a known-failing compile is not re-run; a failing test
> carries its receipt). A `MachineError` is reserved for machine-internal
> failure (a broken invariant, a cycle, an exhausted resource) — the two are
> distinct and a primitive's completion carries which one it is. (The
> preserved `ExecEvent` vocabulary gains an explicit failure variant.)

> r[machine.lifecycle.failure-carries-receipt]
>
> [DESIGN] A failed effect's receipt is as complete as a successful one: the
> read-set that led to the failure is recorded, so re-demanding with the same
> inputs reuses the failure and a changed input re-runs it. "Why did this
> fail, and would it still?" is a lookup.
>
## Ticket liveness

> r[machine.lifecycle.ticket-liveness]
>
> [OPEN → DESIGN] The machine gives no totality guarantee for external effects
> (a network fetch may hang forever), and the scheduler has no clock. Liveness
> is therefore a primitive-declared and caller-requestable property: a ticket
> may carry a deadline or lease, and a cancellation primitive wakes all
> waiters on that demand with a typed `MachineError` (cancelled/timed-out) and
> prevents memo publication. The deadline is enforced by the primitive's own
> runtime (which does have a clock), not by a scheduler poll. OPEN sub-point:
> whether deadlines are mandatory on every external effect or opt-in.

> r[machine.lifecycle.cancellation-poisons-not-memoizes]
>
> [DESIGN] A cancelled or timed-out effect never becomes a memo entry — it is a
> transient `MachineError`, not a receipted result. Re-demand re-runs it.
>
## Freeze / publish atomicity

> r[machine.lifecycle.freeze-transactional]
>
> [DESIGN] Freeze is transactional at the root. If freeze fails after some
> child values are interned or a store slot is allocated, the partial objects
> are unreachable garbage — no memo entry names them, no receipt records them,
> no root points at them. An append-only store cannot roll back by mutation,
> so correctness comes from reachability: a partially-built value that never
> reaches publication is simply never referenced. A failed freeze wakes its
> demand waiters with a `MachineError` and emits a fallback event
> (`machine.obs.loud-fallbacks`).
>
## Poison ordering

> r[machine.lifecycle.poison-is-part-of-publish]
>
> [DESIGN] Poison status is part of the atomic completion/publish decision for
> an effect. The daemon's watch window over an ambient toolchain extends
> through publication, so a mutation-underfoot that would poison a run is
> observed before the result can enter the memo or feed a waiter. A completion
> that arrives after poison is rejected. (This closes the
> launder-a-poisoned-run race.)

> r[machine.lifecycle.post-publish-poison-revokes]
>
> [DESIGN] If poison is nonetheless learned after publication (the watch window
> was exceeded), the machine revokes: the memo entry is invalidated and
> receipts that transitively named it are marked poisoned. Revocation is loud;
> a silently-laundered poisoned result is the failure this rule prevents.
>
## World snapshots

> r[machine.lifecycle.stable-snapshot]
>
> [SETTLED] Every demand and every effect runs against a STABLE SNAPSHOT of
> each world it observes. Within one demand, an observed source does not
> change under it; last-write-wins read-set dedupe
> (`machine.receipt.granularity`) is sound because there is only one value per
> `(argument, path)` within the snapshot. The store is immutable and needs no
> snapshotting; VFS mounts, fetch pins, and ambient-capability views are
> snapshotted by their providing layer for the demand's duration. A source
> that cannot be snapshotted forces `Volatile`
> (`machine.primitive.memo-policy`).
