+++
title = "machine: scheduler"
+++

The demand scheduler: passive data, path-shaped tasks, bounded admission.
Demand is pull — a thousand potential targets cost a thousand map entries,
not a thousand tasks.

> r[machine.scheduler.no-in-program-forcing]
>
> [SETTLED — Vix 101] Everything is lazy; everything is demand-driven. No
> in-program construct forces evaluation: `let p = Point { x: getx(), y:
> gety() }` evaluates neither field — construction binds promises, and `p.x`
> demands only `getx`. Forcing originates solely OUTSIDE the program: the
> holder of the program (most often the vx CLI) demands roots, and demand
> propagates inward through actual consumption — field access, element
> access, pattern match. An eager evaluation appearing anywhere in the
> machine is either a failure of process or a misunderstanding of the
> design; it is never a semantics.

> r[machine.scheduler.task-is-path]
>
> [SETTLED] A task is a path, not a demand. When propagated demand REACHES an
> un-memoized invocation (never before — see
> `machine.scheduler.no-in-program-forcing`), it executes as a call:
> memo-prologue, then direct invocation, same task; memoization happens on
> the unwind. Task boundaries exist at exactly three places: a join (the
> needed demand is already running in another path), an effect wait, and a
> deliberate parallel split (deferred to Stage B).

> r[machine.scheduler.demand-map]
>
> [SETTLED] Scheduler state is a demand map `DemandKey → DemandState`
> (memoized / queued / running). No entry means never demanded — that is the
> demand-driven property, and it is why eager task creation is banned.

> r[machine.scheduler.lifo-admission]
>
> [SETTLED] Runnable work admits LIFO — most recently materialized first — so
> live state scales with dependency depth, not graph width. When the budget is
> full, only work that unblocks a parked task admits; new roots wait.

> r[machine.scheduler.live-budget]
>
> [SETTLED] Admission bounds WIDTH — concurrent independent paths — not depth. A
> task that parks on a fresh child HANDS ITS ADMISSION SLOT to that child, so a
> dependency chain always makes progress regardless of its length. (A strict
> budget counting active+parked would deadlock on any acyclic chain longer than
> the budget: the correctness reviews found this, and it is a correction to the
> original battle-plan model.) Parked frames still count against MEMORY
> accounting; they do not consume a critical-chain admission slot. Sizing rule:
> budget ≈ effect-pool size + independent CPU paths + a join margin.

> r[machine.scheduler.progress-invariant]
>
> [DESIGN] Guaranteed progress: at all times the deepest unfinished demand
> chain holds an admission slot (via slot handoff on park), and every parked
> task waits only on already-admitted work or an external completion. A newly
> materialized dependency of a parked task is an unblocker and admits even at
> full width-budget. The scheduler emits an event if it must admit over budget
> (bounded progress debt), never silently stalls.

> r[machine.scheduler.passive-no-loop]
>
> [SETTLED] The scheduler is passive data mutated through primitive calls
> (`memo_check`, `claim`, `publish`, `join`) from within executing tasks. No
> central loop, no clock, no poll cadence. (Twin of
> `machine.arch.scheduler-is-passive`, stated here for the implementor.)

> r[machine.scheduler.persistent-state]
>
> [SETTLED] Scheduler furniture — demand map, runnable stack, waiters, budget
> — persists across demands and is reused. A park allocates nothing in steady
> state; the ten-Vec boxed pending protocol is the named counter-example.

> r[machine.scheduler.realized-fast-path]
>
> [SETTLED] Operations on realized values — unwrap, coerce, project, invoke-
> target resolution — are inline in both interp and JIT lanes. Parking is
> reserved for genuine pending. The park machinery is the demand loop's
> exception handler, not its main road. (The old machine parked EVERY option
> unwrap as an async request; its "native" unwrap was only the error arm.)

> r[machine.scheduler.slots-lockstep]
>
> [DESIGN] Readiness/awaited state is one `Slots` type maintaining its own
> lockstep invariant, bitset-backed, with no zero-sentinel (zero is a valid
> word). Parallel positional arrays are banned (silent-neighbor corruption).

> r[machine.scheduler.block-on-event]
>
> [SETTLED] Waits block on completion events, never the clock. Poll-plus-sleep
> harvesting is banned. Note: polling vacuously satisfies "no lost wakeup" —
> reviews of this rule must quote the blocking mechanism, not the absence of
> races.

> r[machine.scheduler.completion-resumes-direct]
>
> [DESIGN] An effect completion resumes its parked task directly. Completion
> delivery is typed; a completion-sender's death is a loud typed event, never
> a stringly disconnect error.

> r[machine.scheduler.join-atomic]
>
> [DESIGN] `join(key, waiter)` is one atomic scheduler mutation: under a single
> mutation it either observes the memoized completion and returns it, or
> installs the waiter before any `publish(key)` can drain waiters — no lost
> wakeup between "observe running" and "register." The scheduler maintains a
> wait-for graph; a cycle (A joins B, B joins A) is a typed `MachineError`
> unless the key class explicitly declares recursive/fixpoint semantics.

> r[machine.scheduler.demand-services]
>
> [DESIGN] The scheduler-as-primitive surface includes the demand/call services
> (census class C): invoke, pending alloc/coerce/invoke, tree project, tree
> text, array-map-pending. Each mutates passive scheduler data; none opens a
> private cache or a side channel.

> r[machine.scheduler.observation-recording]
>
> [DESIGN] Capability acquisition journals its observation deduped by content
> hash and emits a timestamped event, so acquiring the same capability twice is
> one journaled fact, observable in the trace.

> r[machine.scheduler.no-shadow-scheduler]
>
> [SETTLED] No suspension machinery beside a substrate that suspends. Weavy
> owns yield/resume; the machine owns admission and bookkeeping only. Async
> vocabulary (await/poll/pending) means actual suspension
> (`machine.arch.one-authority`).

> r[machine.scheduler.replay-is-semantics]
>
> [SETTLED] The semantic model of execution is REPLAY: every execution is
> restartable from its `DemandKey` at any moment, and killing any in-flight
> task is always sound — it loses time, never correctness, and changes no
> observable result. Execution state's canonical form is the memo + demand
> map (content-addressed, persistent, shippable); anything held in a live
> stack is derived. The preconditions are load-bearing rules elsewhere:
> `machine.lifecycle.freeze-transactional` (a killed task's partial work is
> unreachable garbage), `machine.lifecycle.stable-snapshot` (replay sees the
> same world), and `machine.scheduler.tickets-outlive-tasks`. (Resolved by
> Amos, 2026-07-08.)

> r[machine.scheduler.suspension-is-acceleration]
>
> [SETTLED] Executions run as weavy tasks — pending = yield, completion =
> resume; the substrate owns suspension (Law 20) — and a suspended task's
> state is a DISCARDABLE CACHE of replay progress, never the source of truth.
> This is the executor's interp/JIT relationship applied to the scheduler
> (the slow honest mechanism defines semantics; the fast one must be
> observably equivalent), and the store's molten/interned duality applied to
> scheduling state. Task creation remains lazy-on-demand-pull and bounded by
> admission; the model never implies task-per-target. Steady state is
> suspension; restart is the exception path (kill, migrate, evict, crash) —
> which is why the restart-only N² replay tax does not apply.

> r[machine.scheduler.tickets-outlive-tasks]
>
> [SETTLED] Effect tickets are owned by the DEMAND, not the requesting task.
> Killing a task with an in-flight effect does not cancel the effect (unless
> policy explicitly cancels the demand); the replay joins the same in-flight
> ticket or reuses its memoized completion.

> r[machine.scheduler.eviction-is-policy]
>
> [DESIGN] Memory pressure is an eviction policy, not a structural limit: the
> budget may kill parked tasks (least-progressed first, or by policy),
> reclaiming their frames and molten arenas entirely; the killed executions
> re-enter as queued demands and replay later. Parked-task memory is a cache
> with an eviction policy. Migration is the same move across machines: kill
> locally, ship the DemandKey, restart remotely against warm memo.

> r[machine.scheduler.chaos-kill-oracle]
>
> [SETTLED — day one] The replay/suspension equivalence is enforced by a
> standing chaos oracle from R0: a test mode that randomly kills parked and
> running tasks mid-suite and asserts results identical to the unkilled run.
> It runs always (CI-standing, not opt-in); the kill-anytime invariant rots
> silently without it, and every future scheduler feature must pass it.

> r[machine.scheduler.effect-overlap]
>
> [DESIGN] Effect requests are spawned-and-parked; serial inline draining of
> effect queues is banned. An audit-with-receipts enumerates every drain site
> (the old machine's Stage A covered one lane and left another serial —
> reviews must quote sites, not claims).

> r[machine.scheduler.no-test-phase]
>
> [DESIGN] Testing is not a scheduling concept. A test is an exec node
> scheduled by ordinary demand propagation; compilation and testing interleave
> by demand, not phase (testing-as-demand).

> r[machine.scheduler.inner-loop-counters]
>
> [DESIGN] Oracle-enforced counters on pure-solve inner loops: hostcalls per
> iteration = 0, scheduler requests per iteration = 0. These are gate
> criteria, wired through the observability spine from R0.
