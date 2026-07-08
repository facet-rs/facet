+++
title = "machine: scheduler"
+++

The demand scheduler: passive data, path-shaped tasks, bounded admission.
Demand is pull — a thousand potential targets cost a thousand map entries,
not a thousand tasks.

r[machine.scheduler.task-is-path]

[SETTLED] A task is a path, not a demand. A nested un-memoized demand is a
call (memo-prologue, then direct invocation, same task); memoization happens
on the unwind. Task boundaries exist at exactly three places: a join (the
needed demand is already running in another path), an effect wait, and a
deliberate parallel split (deferred to Stage B).

r[machine.scheduler.demand-map]

[SETTLED] Scheduler state is a demand map `DemandKey → DemandState`
(memoized / queued / running). No entry means never demanded — that is the
demand-driven property, and it is why eager task creation is banned.

r[machine.scheduler.lifo-admission]

[SETTLED] Runnable work admits LIFO — most recently materialized first — so
live state scales with dependency depth, not graph width. When the budget is
full, only work that unblocks a parked task admits; new roots wait.

r[machine.scheduler.live-budget]

[SETTLED] Admission is bounded by a live budget counting active AND parked
tasks (parked tasks hold frames and molten state; they are the memory).
Sizing rule: budget ≈ effect-pool size + active CPU paths + a join margin.

r[machine.scheduler.passive-no-loop]

[SETTLED] The scheduler is passive data mutated through primitive calls
(`memo_check`, `claim`, `publish`, `join`) from within executing tasks. No
central loop, no clock, no poll cadence. (Twin of
`machine.arch.scheduler-is-passive`, stated here for the implementor.)

r[machine.scheduler.persistent-state]

[SETTLED] Scheduler furniture — demand map, runnable stack, waiters, budget
— persists across demands and is reused. A park allocates nothing in steady
state; the ten-Vec boxed pending protocol is the named counter-example.

r[machine.scheduler.realized-fast-path]

[SETTLED] Operations on realized values — unwrap, coerce, project, invoke-
target resolution — are inline in both interp and JIT lanes. Parking is
reserved for genuine pending. The park machinery is the demand loop's
exception handler, not its main road. (The old machine parked EVERY option
unwrap as an async request; its "native" unwrap was only the error arm.)

r[machine.scheduler.slots-lockstep]

[DESIGN] Readiness/awaited state is one `Slots` type maintaining its own
lockstep invariant, bitset-backed, with no zero-sentinel (zero is a valid
word). Parallel positional arrays are banned (silent-neighbor corruption).

r[machine.scheduler.block-on-event]

[SETTLED] Waits block on completion events, never the clock. Poll-plus-sleep
harvesting is banned. Note: polling vacuously satisfies "no lost wakeup" —
reviews of this rule must quote the blocking mechanism, not the absence of
races.

r[machine.scheduler.completion-resumes-direct]

[DESIGN] An effect completion resumes its parked task directly. Completion
delivery is typed; a completion-sender's death is a loud typed event, never
a stringly disconnect error.

r[machine.scheduler.no-shadow-scheduler]

[SETTLED] No suspension machinery beside a substrate that suspends. Weavy
owns yield/resume; the machine owns admission and bookkeeping only. Async
vocabulary (await/poll/pending) means actual suspension
(`machine.arch.one-authority`).

r[machine.scheduler.executions-as-weavy-tasks]

[OPEN — decision 1] Whether path executions run AS weavy tasks (pending =
yield, completion = resume; the burst protocol absorbed by the substrate)
requires Amos's ruling, with the constraint already set: task creation is
lazy-on-demand-pull and bounded by admission — the model must never imply
task-per-target. If ruled no, a written justification per Law 20 is
required.

r[machine.scheduler.effect-overlap]

[DESIGN] Effect requests are spawned-and-parked; serial inline draining of
effect queues is banned. An audit-with-receipts enumerates every drain site
(the old machine's Stage A covered one lane and left another serial —
reviews must quote sites, not claims).

r[machine.scheduler.no-test-phase]

[DESIGN] Testing is not a scheduling concept. A test is an exec node
scheduled by ordinary demand propagation; compilation and testing interleave
by demand, not phase (testing-as-demand).

r[machine.scheduler.inner-loop-counters]

[DESIGN] Oracle-enforced counters on pure-solve inner loops: hostcalls per
iteration = 0, scheduler requests per iteration = 0. These are gate
criteria, wired through the observability spine from R0.
