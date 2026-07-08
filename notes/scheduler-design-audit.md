+++
title = "Scheduler design audit: does the vix demand driver meet its stated concurrency goals?"
date = "2026-07-08"
+++

# Scheduler design audit

*Read-only audit, Amos-ordered. Prompt: "the fact that there isn't already some
free concurrency in rodin.vix makes me suspicious of the vix
implementation/scheduler and whether it meets its stated design goals." Evidence:
stax thread tables show solve workloads running ONE thread hot, all others idle.*

## VERDICT (terse)

**Classification (c): in between — but the "in between" resolves cleanly, and it
is NOT a contradiction.**

The design documents promise a demand-driven **async-runtime** evaluator whose
endgame is **workers + work-stealing**, with structural (joint-demand) fan-out as
the semantic default. But the *same* documents that promise workers explicitly
stage them as **"single-threaded first"** — width arrives *later*, "by sharing the
queue." The driver as it exists today (`vix/src/machine/driver.rs`) faithfully
implements exactly that promised **first rung**: a real work-list scheduler (not
host-stack recursion), with joint-demand fan-out structurally available, memo /
in-flight-dedupe / park-and-wake all present — and **zero worker threads**.

So the "one thread hot" observation is **fully explained by the design as
written** and is not a bug against it. What it *contradicts* is Amos's expectation
of *free concurrency now*. Three compounding reasons that expectation is ahead of
the implemented stage:

1. **No worker pool exists.** Pure-demand evaluation (bursting the interp/JIT
   lane) is single-threaded by construction. The design defers this ("workers/
   stealing arrive by sharing the queue" — machine-lowering.md:268). Solve is
   CPU-bound and in-process, so it pins one thread. Expected, per design.
2. **Even the structural fan-out that DOES exist is defeated by sequential source
   shapes.** The driver *can* spawn N children from one burst (joint demand), but
   a `fold`/tail-recursive accumulator emits `INVOKE, Await, INVOKE, Await…` —
   one child at a time. The most *recent* landed work (redesign/8-intra-demand-loops.md,
   `rodin` 2026-07-07) deliberately collapses tail iteration to **one frame, one
   spawn, zero per iteration** — i.e. it pushes solve-shaped workloads *toward* the
   straight line Amos observed. That is a feature (molten accumulators, no
   per-iteration intern), but it is the opposite of width.
3. **The one concurrency that is cheap and already-promised — overlapping
   exec/fetch runs — is left on the table.** The driver drains exec requests
   **inline and synchronously** (`self.execute_request(req)` fills the slot
   immediately, driver.rs:2944), never parking on a run to go do other runnable
   work. So even I/O-bound builds don't overlap in this driver.

Bottom line: **the implementation meets the stated design's *first stage*; it
predates (does not contradict) the width the design defers.** The suspicion is
directionally right — there is no free concurrency — but the cause is "the width
rung was never built, and recent work optimized *toward* the straight line," not
"the scheduler betrays its design."

---

## 1. STATED GOALS — what the design promises, verbatim

The corpus splits into an **aspiration layer** (what the evaluator *is*, endgame)
and an **operational layer** (how it's *staged*). Both matter for the verdict.

### Aspiration: async runtime, workers, work-stealing, parallelism-by-default

`engine-demand-semantics.md` §A5 (2026-07-04, "THE TWO-EVALUATOR ERA ENDS"):

> "THE evaluator of vix is the weavy-async lowering: this document's demand graph,
> **driven as an async runtime (workers, work-stealing — suspend-ABI ruling)**,
> interpreter lane first."

`engine-demand-semantics.md` §3.6 (strictness points):

> "`units.map(f).collect()` — collect demands the collection's Identity, which
> demands each element's Identity JOINTLY (**parallel fan-out is discovered, not
> annotated**); map produces element Thunks (Call nodes), so the fan-out is a
> batch of memo boundaries."

`vix-synthesis.md:375, 380-381` (the graph-demand-engine invariants — one is
literally a passing test):

> "independent-inputs-resolve-**concurrently** (JOINT Need — Apply demands all
> inputs at once, **~max not sum**)"

> "nodes declare Need(set) JOINTLY (**parallelism default**); aggregation BATCHES
> demand at fused-node boundaries (perf, not semantics)."

`vix-synthesis.md:395` (the "free consequences" of demand-driven):

> "map spawns ALL compiles before collect awaits any (**parallel fan-out from
> sequential source**)"

### Operational staging: single-threaded FIRST; workers come later

`machine-lowering.md` §"The driver (the demand scheduler)" (lines 264-271) — the
load-bearing quote for the whole verdict:

> "Owns: the demand worklist, the memo store, the journal, the task set
> (running/parked), the readiness plumbing, the store.
> **Single-threaded first, written work-queue-shaped: workers/stealing arrive by
> sharing the queue** (legal: canonical order makes schedules unobservable).
> Persistence only at content-addressed boundaries: memo, journal, lowerings.
> Arena and worklists die with the eval."

`machine-lowering.md` §"Demand edge → await input" (lines 42-45):

> "JOINT NEED at memo boundaries is a SCHEDULER policy: when a body's input set is
> statically known, the driver demands the batch before entering, so bodies don't
> start-park-start-park. Policy about when to begin; never spawning undemanded
> work."

`engine-demand-semantics.md` §A6 (the discipline that keeps width from becoming
eager spawning):

> "**Suspension is only for the started-and-blocked.** A computation that began and
> cannot make forward progress suspends. An undemanded node is pure data — no
> future, no frame, no cost (graphs have millions of nodes). Never
> spawn-everything. Joint-demand batching at memo boundaries is a SCHEDULER POLICY
> about when to begin — not a default, never a return to spawning."

And on exec concurrency specifically, `vix-synthesis.md:721`:

> "streams genuinely concurrent runs; the local oracle is synchronous, so [it]…"
> — i.e. concurrent *runs* are a fleet/endgame property; the local evaluator is
> acknowledged synchronous.

**What is promised, distilled:** (a) demand-driven with joint/parallel fan-out as
the *semantic* default — width is *discovered*, not annotated; (b) the runtime
*shape* is async (suspend/resume, park/wake); (c) actual multi-thread execution is
**explicitly staged after** a single-threaded first cut, made safe by canonical
order ("schedules unobservable") and content-addressed persistence boundaries.

---

## 2. ACTUAL ARCHITECTURE — how the driver evaluates demands today

### It is a scheduler, not a call stack

`Driver::demand` (driver.rs:2806) is **not** depth-first host-stack recursion. It
is a hand-rolled **work-list driver** over an arena of suspendable executions
(driver.rs:2836-2847):

```rust
let mut executions: Vec<Option<Execution>> = Vec::new();
let mut waiters: IdentityHashMap<CanonMemoKey, Vec<(usize, usize)>> = …;
let mut in_flight = InFlightInvocations::default();
let mut runnable: Vec<usize> = Vec::new();

let root = self.spawn(&mut executions, fn_ref, key.clone(), &args)?;
in_flight.started(key.clone());
runnable.push(root);

while let Some(ix) = runnable.pop() {          // ← the scheduler loop
    let mut exec = executions[ix].take()…;
    let requests = self.burst(&mut exec, ix);  // run one execution until it parks
    …
}
```

All four scheduler ingredients are present:

- **Ready queue** — `runnable: Vec<usize>`, drained by `pop()` (**LIFO**; see path
  notes below).
- **Multiple in-flight executions** — the `executions` arena holds every
  spawned-but-not-finished task simultaneously; the demand graph's *state lives in
  the arena*, exactly as `engine-demand-semantics.md` §6 requires ("recursion IS
  demand propagation, the arena IS the state"), so nodes are plain data and the
  host stack is not the driver.
- **Park + precise wake** — on a memo miss the caller registers as a waiter
  (driver.rs:3150) and is re-pushed to `runnable` only when the slot it *parked
  on* becomes ready (driver.rs:3172-3178): "never re-poll a blocked task — the
  waker-precision rule at driver level." One completion feeds every parked waiter
  (driver.rs:2898-2916): "One completion, many resumptions."
- **In-flight dedupe** — `in_flight` keyed by `CanonMemoKey`; a second demand for
  a key already running does **not** spawn — it just adds a waiter
  (`already_running` at driver.rs:3145-3164).

Three spawn-suppression mechanisms (module doc, driver.rs:1-27): **MEMO HIT**
(sync slot-fill, no task), **UNDEMANDED** (never materialized), **PARKED** (the
only mechanism that costs a frame).

### Where the await machinery is exercised

The `ready`/`awaited` arrays + `Op::Await` protocol (weavy/src/task.rs:485-490,
the consumable-ready-token fix from redesign/8) is exercised **anywhere a body
awaits an input slot** — not only at exec nodes. Every memo-boundary INVOKE
(driver.rs:3087-3166), every projection/text-projection request, every
coercion/option-unwrap, and every exec/fetch/doc-parse run fills an input slot
that a body `Op::Await`s. Park happens at the *first unready await* in a burst
(driver.rs:3172, `parked_input`). So await is the universal join primitive across
pure-demand evaluation, not an exec-only device.

### Does anything ever interleave two independent pure demands?

**Structurally yes; temporally no.** A single burst can register *multiple* INVOKE
requests before it parks: the `invoke` host closure pushes each call into a
`requests: Vec<InvokeRequest>` (driver.rs:3239, 3299) reading successive slots of
`invoke_region` (driver.rs:3268, `i * 8`), and the body runs straight-line until
the first `Op::Await` it can't satisfy. So a joint-demand fan-out (`map`→`collect`,
lowered as `INVOKE…INVOKE…Await…Await`; cf. the two back-to-back `Op::Await
{input:0}` / `{input:1}` in the driver's own tests at driver.rs:12131-12132)
spawns **N executions at once**, all live in the arena, all on `runnable`.

But they are processed **one burst at a time on the calling thread**. `runnable.pop()`
→ `self.burst(...)` runs entirely on one thread; `LaneRuntime::spawn` (driver.rs:619)
builds a `Task`/`JitTask` and `burst` steps it via `LaneTask::advance` →
`task.run_hosted` (driver.rs:666) synchronously. **No `thread::spawn`, no rayon, no
`tokio::spawn`, no work-stealing anywhere in the demand path** — verified by grep
over `vix/src` + `weavy/src`: the only `thread::spawn` sites are the *exec
backends* (`real_process.rs:49`, `rpc_process.rs:109` — one OS thread per external
subprocess, unrelated to demand parallelism), and the only `tokio::spawn` is a
test (`weavy/src/task.rs:1172`). Two independent pure demands therefore **coexist
and are cooperatively scheduled, but never run simultaneously** — "interleave"
holds only in the weak sense of arena coexistence + LIFO ordering.

### The async substrate exists but the driver bypasses it for real work

`weavy` *does* carry an async harness — `impl Future for TaskExec` (task.rs:587),
`#[tokio::test(flavor = "multi_thread", worker_threads = 2)]` cases. The vix
driver does **not** use it: it re-implements the worklist as a synchronous
`while let Some(ix) = runnable.pop()` loop, owning the readiness arrays itself
(`exec.ready`/`exec.awaited`) and stepping the lane through the `Advance` seam.
This is *consistent with* the design (machine-lowering.md:37-38: "the machine
driver owns per-task readiness/value arrays (exactly TaskExec's shape — the
Advance seam holds either lane)") — the driver reusing the `Advance` seam is the
spec, `TaskExec`/tokio is an alternate harness. It is not the contradiction; it's
just why the async-runtime *word* in §A5 doesn't translate to threads on the
ground yet.

### Exec runs are inline-synchronous — the cheapest concurrency is unused

In the `Burst::Pending` handler, exec/fetch/doc-parse/crate-archive requests are
drained by calling the primitive **inline** and filling the slot **immediately**
(driver.rs:2944-2955):

```rust
for req in exec_requests {
    …
    match self.execute_request(req) {
        Ok((input_slot, value)) => { exec.ready[input_slot] = true; exec.awaited[input_slot] = value; }
        Err(err) => return Err(err),
    }
}
```

The caller is **not** parked on the run; the driver does **not** pop other
`runnable` work while the run is outstanding. So even though `real_process` runs
each subprocess on its own OS thread, the driver blocks on it and serializes runs.
The exec seam is a memo-boundary-shaped primitive ("a run's completion feeds await
slots exactly like a node's" — machine-lowering.md:46-49) but is currently wired
synchronously rather than spawn-and-park.

---

## 3. VERDICT — meet / predate / contradict

**Predate, cleanly. Not a contradiction.**

| Design promise | Implemented today? |
|---|---|
| Demand-driven, nothing forces locally | **Yes** — memo/park/suppression enforced + trace-asserted |
| Work-queue-shaped scheduler (not recursion) | **Yes** — `runnable` worklist, arena of suspendable execs, precise wake |
| Joint/parallel fan-out *discovered* | **Structurally yes** — multi-INVOKE-per-burst → N spawns; **but** defeated by sequential source shapes, and recent tail-loop work collapses iteration to one frame |
| In-flight dedupe at the memo/invocation boundary | **Yes** — `in_flight` + `already_running` waiter fold |
| "Single-threaded first" | **Yes — this is the current rung** |
| Workers / work-stealing ("by sharing the queue") | **No — explicitly deferred by the design** |
| Concurrent exec/fetch runs | **No — exec drained inline-synchronously** (design wants this at fleet/endgame; local acknowledged synchronous, vix-synthesis:721) |

Neither side is violated. The design **promised width as a later rung and a
single-threaded first cut**; the driver **is** that first cut, built to the spec's
shape (worklist, arena, park/wake, dedupe, canonical order). Amos's expectation of
*free concurrency now* is (c)-flavored: the semantic *default* is parallel
(§3.6, vix-synthesis:381), which invites the expectation, but the *execution* rung
that would cash it out was deferred — and the newest work optimized toward the
straight line. The stax "one thread hot" reading is the correct, expected
signature of the built stage.

---

## 4. THE PATH — what demand-width would require of the current architecture

Two independent axes. The first is small and buys real wall-clock wins for *builds*
(the rodin.vix case). The second is the design's deferred rung and buys wins for
*pure solve*. The current shapes (per-execution molten ownership, `in_flight`
dedupe, content-addressed store as the single shared-mutation point) are already
the right substrate for both.

### Stage A — single-thread cooperative interleaving of runs (build concurrency)

The machinery for this **already exists**; only the exec/fetch draining is wired
wrong. Convert the inline exec/fetch/doc-parse/crate-archive arms from
"call-and-fill-slot" (driver.rs:2944-3006) into **spawn-and-park**, reusing the
exact waiter/ready path that INVOKE already uses:

- Add a pending-runs registry keyed like `in_flight` (a run → parked waiters map).
- On an exec/fetch request: kick the run (the backends already run off-thread —
  `real_process.rs:49`), register the caller as a waiter on its slot, and **do not
  re-push** the caller unless its `parked_input` is otherwise ready. Keep popping
  `runnable`.
- Add a poll/collect step in the driver loop that harvests finished runs and wakes
  their waiters (mirroring the INVOKE-completion wake at driver.rs:2898-2916).

**Components touched:** the four inline request arms in `Burst::Pending`; a new
pending-run registry + wake step; the driver loop's termination condition (drain
until `runnable` empty **and** no runs outstanding). **Contained to driver.rs.**
This delivers overlapping subprocess/network runs — actual "free concurrency" for
rodin builds — with **no threading added to the demand graph** and no store
sharing. It does **not** help pure solve (solve is CPU-bound in-process).

### Stage B — multi-worker pure-demand parallelism (the deferred rung)

This is the design's "workers/stealing arrive by sharing the queue." Share
`runnable` across worker threads; each worker pops an execution and bursts it to
its next park. The safety story is already latent in the data model:

- **Molten single-ownership = the safety story.** Each `Execution` owns its
  `molten: MoltenStore` (driver.rs:583) — molten values are single-owner and are
  only ever published by **interning into the content-addressed store at
  completion** (driver.rs:2860-2873). So a worker can burst an execution touching
  only *its own* molten arena + read-only shared lowerings; nothing another worker
  can see mutates mid-burst. This is exactly why the design can say "canonical
  order makes schedules unobservable" (machine-lowering.md:269).
- **`in_flight` map = the dedupe/serialization point.** Two workers demanding the
  same key must resolve to one spawn + one waiter — this is *already* the
  `already_running` branch (driver.rs:3145-3164); it needs to become **atomic**
  (compare-and-insert on a concurrent `in_flight`), and the loser parks.
- **Shared mutable state that must go concurrent/sharded:** `self.memo`
  (`HashMap`), `self.store` (`RefCell<…>`), `waiters`, and the `in_flight` set are
  today single-owner (`&mut self` / `RefCell`). These become the sharing surface:
  a sharded/locked content-addressed store (dedup at insert is already the
  `deduped` bool it returns), a concurrent memo, a concurrent in-flight/waiter
  map. The `event_sink`/trace must become thread-safe (or per-worker + merged in
  canonical order).
- **Worklist discipline:** today `runnable` is a LIFO `Vec` (`pop()`). Work-stealing
  wants per-worker deques (steal from the far end) — the design's literal "sharing
  the queue."

**Components touched:** the `Driver` struct's shared-state field types
(memo/store/in_flight/waiters → concurrent or sharded); the `demand` loop → a
worker pool over a shared/stealable worklist; the store's `RefCell` → sync
interior; the event sink → `Send`-able. **`Execution.molten` stays single-owner —
that is the whole point and needs no change.** Larger surface than Stage A, but no
new *invariants*: purity + canonical order + content-addressed publish already
license it; the code just has to stop assuming one owner.

### Ordering note

Stage A is the high-leverage move for the *specific* complaint (rodin builds show
no concurrency): builds are run-bound, and Stage A overlaps runs with a driver.rs-local
change. Stage B is required to make *solve* (the CPU-bound, one-thread-hot
workload in the stax tables) go wide, and it is the design's explicitly-deferred
rung — worth doing against the shape the design already picked, not a redesign.
