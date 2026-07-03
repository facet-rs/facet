# Weavy async — copy-and-patch as a Rust `Future`

Spike proving the prerequisite for lowering vix's demand-driven evaluation to
weavy. Once the IR + Poll ABI land, this graduates into `weavy::jit::async` and
the spike is deleted; the keeper is this mechanism.

## Why this exists

Vix is demand-driven: *nothing forces locally* — application, member access,
operators, conditionals are all graph nodes; demand backpropagates from selected
outputs. A node that awaits its inputs **is a future**; the demand driver **is an
executor**; a cross-executor part landing over vox **is a woken waker**. So the
evaluation substrate needs suspend points. Coroutines were tried and are
impractical (parallel-stack discipline). Rust async is the right shape because
the things vix awaits are *already* tokio futures: an exec flush, a `fetch_path`,
a producing handle.

## The load-bearing insight

A copy-and-patch chain already keeps its state **off the C stack**:

- `Ctx` is an explicit `#[repr(C)]` struct — the immediate cursor (`prog`), the
  operand-stack pointer (`sp`), and host cells. The operand stack is a separate
  buffer, not the C stack.
- The `become` (guaranteed-tail-call) discipline means **no stencil holds live
  C-stack state across its continuation**. Each stencil either tail-calls the
  next (`become weavy_cont`) or returns; it never has a live frame waiting below
  a call.

Therefore the whole chain runs in **one** stack frame — the driver's call.
A stencil can **suspend by simply returning up**: the C stack unwinds to the
driver, and nothing is lost because the state was never on it. Resume =
**re-enter at a saved chain offset** (`NativeProgram::chain_fn`).

This is not a new mechanism. It is the two-successor **type-speculation guard
stencil** with the slow path repurposed:

| guard stencil            | await stencil               |
| ------------------------ | --------------------------- |
| bet holds → `weavy_cont` | ready → `weavy_cont`        |
| bet fails → `weavy_deopt`| pending → return (suspend)  |

## The state machine (phase 2: N suspend points)

A stencil can't know its own address, so the **compiler bakes** each await's
resume offset (its own chain offset) and its await index into the immediate
stream. The `await` stencil:

- **ready**: consume the two immediates, push `awaited[index]`, tail-call next.
- **pending**: write `resume = own offset` and `await_index = index` into `Ctx`,
  set `suspended = 1`, and **return** — *without advancing `prog`*, so a resume
  re-reads the same immediates (idempotent per suspend point).

Readiness and values are **host arrays** indexed by await index. The driver
polls every unresolved input each turn, so independent awaits resolve
**concurrently** even though the chain visits them in program order.

### The driver (`WeavyExec: Future`)

Each `poll`:

1. Drive every unresolved input future; when one lands, arm `ready[i]` /
   `awaited[i]`. Its waker will re-poll us.
2. If parked and the await we're *blocked on* still isn't ready, return
   `Pending` **without re-entering** the chain — a wakeup from some other input
   can't let this suspend point proceed. (In the real engine, each node
   registers only on its own input; the single-driver spike restores that
   precision with this guard.)
3. Enter the chain — root on the first poll, `resume_offset` thereafter. It runs
   to `DONE` (`Poll::Ready`) or returns having set `suspended` (`Poll::Pending`).

The suspended cursor (`sp_len`, `prog`) **persists in the driver across polls** —
off the C stack, which is exactly why it survives the unwind.

## Debuggability (first-class, not an afterthought)

The production engine must show *where* a demand graph is parked and *on what*:

- `WeavyExec::trace: Vec<SuspendEvent>` — the timeline: which await parked, at
  what operand-stack depth, in order.
- `WeavyExec::suspension() -> Option<Suspension>` — the live parked state: the
  await index, the resume offset, and a snapshot of the operand stack below the
  suspend point.

Tests assert these directly (e.g. "while parked on the await, the stack holds
exactly `[30]`").

## What's proven (tests/suspend.rs, over a real multi-thread tokio runtime)

- single await across a suspend → 42, parked once at await #0 with `[40]` on the
  stack;
- ready input never suspends (native fast path);
- two awaits park twice and resume in program order;
- independent awaits resolve concurrently: await #1 lands while parked on #0, so
  the resume sails past #1 — exactly one park;
- the suspended state is inspectable while parked.

## Next (toward graduation)

1. weavy IR gains a `suspend`/`await` op + the `Poll` ABI; the extraction and
   chain assembly move behind `weavy::jit::async`.
2. Values become real vix `Value`s (guard-stencil territory: unbox on ready).
3. The graph stage lowers AST+env → nodes; `Need(set)` = joint `.await` on input
   nodes; aggregation batches demand at fused-node boundaries.
