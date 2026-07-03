//! The async lane: a JIT'd copy-and-patch chain that SUSPENDS at await points
//! and RESUMES — driven as a real Rust [`Future`], awaiting real Rust futures.
//!
//! This is the evaluation substrate for a demand-driven language (vix): a node
//! that awaits its inputs IS a future, the demand driver IS an executor, and a
//! cross-executor value landing over the network IS a woken waker. Coroutines
//! were the wrong shape (a parallel-stack discipline); Rust async is right
//! because the awaited things — an exec flush, a remote fetch, a producing
//! handle — are already futures.
//!
//! # Why copy-and-patch can suspend at all
//!
//! A chain keeps its state OFF the C stack: [`Ctx`] is an explicit struct — the
//! immediate cursor, the operand-stack pointer, and host cells; the operand
//! stack is a separate buffer. The guaranteed-tail-call discipline (`become`,
//! see `build.rs`) means no stencil holds live C-stack state across its
//! continuation. So the whole chain runs in ONE stack frame — the driver's
//! call — and a stencil can SUSPEND by simply returning up: the C stack unwinds
//! to the driver and nothing is lost, because the state was never on it. Resume
//! = re-enter at a saved chain offset ([`NativeProgram::chain_fn`]).
//!
//! This is not a new mechanism. It is the two-successor type-speculation guard
//! stencil with the slow path repurposed:
//!
//! | guard stencil            | await stencil              |
//! | ------------------------ | -------------------------- |
//! | bet holds → `weavy_cont` | ready → `weavy_cont`       |
//! | bet fails → `weavy_deopt`| pending → return (suspend) |
//!
//! # The state machine (N suspend points)
//!
//! A stencil can't know its own address, so [`compile`] BAKES each await's
//! resume offset (its own chain offset) and index into the immediate stream.
//! On suspend the await writes both into [`Ctx`] (so the driver re-enters
//! exactly there and learns which future parked it) and returns WITHOUT
//! advancing `prog`, so a resume re-reads the same immediates — idempotent per
//! suspend point. Readiness/values are host arrays indexed by await index, and
//! the driver polls every unresolved input each turn, so independent awaits
//! resolve CONCURRENTLY even though the chain visits them in program order.
//!
//! # Debuggability (first-class)
//!
//! The production engine must show WHERE a demand graph is parked and ON WHAT.
//! [`AsyncExec::trace`] is the suspension timeline (which await parked, at what
//! operand-stack depth, in order); [`AsyncExec::suspension`] exposes the live
//! parked state (await index, resume offset, operand-stack snapshot). Tests
//! assert these directly.
//!
//! # Scope (v1)
//!
//! The operand type is `i64` — the canonical async lane, enough to prove and
//! exercise the substrate. Generalizing the operand to a tagged `Value` (guard-
//! stencil territory: unbox on ready) and wiring a `suspend` node into
//! [`crate::ir`] are the follow-up slices; the suspend/resume PROTOCOL here is
//! the reusable part.

#[cfg(feature = "jit")]
use core::future::Future;
#[cfg(feature = "jit")]
use core::pin::Pin;
#[cfg(feature = "jit")]
use core::task::{Context, Poll};

#[cfg(feature = "jit")]
use crate::jit::{NativeProgram, StencilLayout, async_stencils};

/// Threaded state — MUST match `Ctx` in `stencils/async_ops.rs` (repr C, order).
#[cfg(feature = "jit")]
#[repr(C)]
struct Ctx {
    prog: *const u64,
    sp: *mut i64,
    ready: *const i64,
    awaited: *const i64,
    resume: *mut u64,
    await_index: *mut u64,
    suspended: *mut i64,
}

/// One op in an async program. `Await` points are numbered in program order;
/// the caller supplies one future per await, in the same order.
#[cfg(feature = "jit")]
#[derive(Clone, Copy, Debug)]
pub enum Op {
    /// Push an immediate onto the operand stack.
    Push(i64),
    /// Suspend point: await the next external input, pushing its value when
    /// ready.
    Await,
    /// Pop two, push their sum.
    Add,
    /// Pop two, push their product.
    Mul,
}

/// A compiled async program: the executable chain plus the chain offset of each
/// await (its resume point, also baked into the immediate stream).
#[cfg(feature = "jit")]
pub struct AsyncProgram {
    native: NativeProgram,
    await_offsets: Vec<usize>,
}

#[cfg(feature = "jit")]
impl AsyncProgram {
    /// Number of await (suspend) points in the chain.
    pub fn await_count(&self) -> usize {
        self.await_offsets.len()
    }

    /// Chain offset of await #i (its resume point) — useful for debug tooling
    /// mapping a parked `resume_offset` back to a suspend point.
    pub fn await_offset(&self, index: usize) -> Option<usize> {
        self.await_offsets.get(index).copied()
    }
}

/// Whether the async lane compiled for this target (native copy-and-patch).
#[cfg(feature = "jit")]
pub fn available() -> bool {
    !async_stencils::PUSH.is_empty() && crate::jit::NATIVE_COPY_PATCH_AVAILABLE
}

/// Assemble ops into a suspendable copy-and-patch chain. Each await bakes its
/// own chain offset and index into the immediate stream (the resume machinery).
/// Returns `None` if the async lane is unavailable on this target.
#[cfg(feature = "jit")]
pub fn compile(ops: &[Op]) -> Option<AsyncProgram> {
    if !available() {
        return None;
    }
    let mut layout = StencilLayout::new();
    let root = layout.start_chain();
    let mut sites: Vec<(usize, &'static [usize])> = Vec::new();
    let mut await_offsets = Vec::new();

    for op in ops {
        if let Op::Push(n) = op {
            layout.push_prog_word(root.prog_index, *n as u64);
        }
        let (bytes, cont): (&[u8], &'static [usize]) = match op {
            Op::Push(_) => (async_stencils::PUSH, async_stencils::PUSH_CONT),
            Op::Await => (async_stencils::AWAIT, async_stencils::AWAIT_CONT),
            Op::Add => (async_stencils::ADD, async_stencils::ADD_CONT),
            Op::Mul => (async_stencils::MUL, async_stencils::MUL_CONT),
        };
        let start = layout.emit_stencil(bytes);
        if matches!(op, Op::Await) {
            let index = await_offsets.len();
            layout.push_prog_word(root.prog_index, start as u64);
            layout.push_prog_word(root.prog_index, index as u64);
            await_offsets.push(start);
        }
        sites.push((start, cont));
    }

    let done = layout.emit_stencil(async_stencils::DONE);
    for i in 0..sites.len() {
        let (start, cont) = sites[i];
        let target = sites.get(i + 1).map(|(s, _)| *s).unwrap_or(done);
        for &rel in cont {
            layout.patch_continuation(start + rel, target);
        }
    }
    Some(AsyncProgram {
        native: NativeProgram::new(layout, root),
        await_offsets,
    })
}

/// A recorded suspension — the debuggable timeline of where the chain parked.
#[cfg(feature = "jit")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SuspendEvent {
    /// Which await point parked the chain.
    pub await_index: usize,
    /// Operand-stack depth at the moment of suspension (what's computed so far).
    pub stack_depth: usize,
}

/// The live state of a currently-suspended chain (for inspection/debugging).
#[cfg(feature = "jit")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suspension {
    /// The await point the chain is parked on.
    pub await_index: usize,
    /// The chain offset the driver will re-enter on resume.
    pub resume_offset: usize,
    /// Snapshot of the operand stack below the suspend point.
    pub stack: Vec<i64>,
}

/// The DRIVER: a Rust [`Future`] over a JIT'd chain with N awaits. One inner
/// future per await; each poll drives every unresolved input, then enters the
/// chain (root first, resume offset thereafter) until it completes or parks.
///
/// The driver depends only on `core::future` — no async runtime. Provide any
/// executor (the tests use tokio) and any input futures (in production: vox
/// streams, remote fetches).
#[cfg(feature = "jit")]
pub struct AsyncExec {
    program: AsyncProgram,
    inners: Vec<Pin<Box<dyn Future<Output = i64>>>>,
    resolved: Vec<bool>,
    ready: Vec<i64>,
    awaited: Vec<i64>,
    stack: Vec<i64>,
    // Suspended cursor — the live state, kept off the C stack, which is why it
    // survives the unwind-to-driver.
    sp_len: usize,
    prog: *const u64,
    resume_offset: usize,
    started: bool,
    // Scratch cells the stencils write on suspend.
    suspended: i64,
    resume_scratch: u64,
    await_index_scratch: u64,
    /// The debuggable suspension timeline (append-only).
    pub trace: Vec<SuspendEvent>,
}

#[cfg(feature = "jit")]
impl AsyncExec {
    /// One input future per await point, in program order.
    ///
    /// # Panics
    /// If `inners.len()` doesn't match the program's await count.
    pub fn new(
        program: AsyncProgram,
        inners: Vec<Pin<Box<dyn Future<Output = i64>>>>,
    ) -> Self {
        assert_eq!(
            inners.len(),
            program.await_count(),
            "one input future per await point"
        );
        let n = inners.len();
        let prog = program.native.entry_prog();
        AsyncExec {
            program,
            inners,
            resolved: vec![false; n],
            ready: vec![0; n],
            awaited: vec![0; n],
            stack: vec![0; 256],
            sp_len: 0,
            prog,
            resume_offset: 0,
            started: false,
            suspended: 0,
            resume_scratch: 0,
            await_index_scratch: 0,
            trace: Vec::new(),
        }
    }

    /// The live suspended state, if the chain is currently parked.
    pub fn suspension(&self) -> Option<Suspension> {
        (self.started && self.suspended != 0).then(|| Suspension {
            await_index: self.await_index_scratch as usize,
            resume_offset: self.resume_scratch as usize,
            stack: self.stack[..self.sp_len].to_vec(),
        })
    }

    fn run_from(&mut self, offset: usize) -> Option<i64> {
        self.suspended = 0;
        let base = self.stack.as_mut_ptr();
        let mut ctx = Ctx {
            prog: self.prog,
            sp: unsafe { base.add(self.sp_len) },
            ready: self.ready.as_ptr(),
            awaited: self.awaited.as_ptr(),
            resume: &mut self.resume_scratch,
            await_index: &mut self.await_index_scratch,
            suspended: &mut self.suspended,
        };
        // SAFETY: `offset` is a chain offset in this program; the copied code
        // uses the `extern "C" fn(*mut Ctx)` ABI the stencils were built with.
        let entry = unsafe { self.program.native.chain_fn::<Ctx>(offset) };
        unsafe { entry(&mut ctx) };
        self.sp_len = (ctx.sp as usize - base as usize) / size_of::<i64>();
        self.prog = ctx.prog;
        if self.suspended != 0 {
            self.resume_offset = self.resume_scratch as usize;
            None
        } else {
            Some(self.stack[self.sp_len - 1])
        }
    }
}

#[cfg(feature = "jit")]
impl Future for AsyncExec {
    type Output = i64;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<i64> {
        let this = &mut *self;

        // Drive EVERY unresolved input; independent awaits make progress
        // concurrently, so a resume can sail past awaits that landed early.
        for i in 0..this.inners.len() {
            if !this.resolved[i]
                && let Poll::Ready(value) = this.inners[i].as_mut().poll(cx)
            {
                this.awaited[i] = value;
                this.ready[i] = 1;
                this.resolved[i] = true;
            }
        }

        // Parked and the await we're BLOCKED on still isn't ready: don't
        // re-enter — a wakeup from some other input can't advance this suspend
        // point. (The real engine registers each node only on its own input;
        // the single-driver lane restores that precision with this guard.)
        if this.started && this.suspended != 0 {
            let blocked_on = this.await_index_scratch as usize;
            if this.ready[blocked_on] == 0 {
                return Poll::Pending;
            }
        }

        let entry = if this.started {
            this.resume_offset
        } else {
            this.started = true;
            0
        };
        match this.run_from(entry) {
            Some(result) => Poll::Ready(result),
            None => {
                this.trace.push(SuspendEvent {
                    await_index: this.await_index_scratch as usize,
                    stack_depth: this.sp_len,
                });
                Poll::Pending
            }
        }
    }
}

#[cfg(all(test, feature = "jit"))]
mod tests {
    use super::*;
    use std::time::Duration;

    fn later(value: i64, ms: u64) -> Pin<Box<dyn Future<Output = i64>>> {
        Box::pin(async move {
            tokio::time::sleep(Duration::from_millis(ms)).await;
            value
        })
    }

    async fn drive(mut exec: AsyncExec) -> (i64, Vec<SuspendEvent>) {
        let result = core::future::poll_fn(|cx| Pin::new(&mut exec).poll(cx)).await;
        (result, exec.trace.clone())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn single_await_across_a_suspend() {
        if !available() {
            return;
        }
        let program = compile(&[Op::Push(40), Op::Await, Op::Add]).unwrap();
        assert_eq!(program.await_count(), 1);
        let (result, trace) = drive(AsyncExec::new(program, vec![later(2, 40)])).await;
        assert_eq!(result, 42);
        assert_eq!(trace, vec![SuspendEvent { await_index: 0, stack_depth: 1 }]);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ready_input_never_suspends() {
        if !available() {
            return;
        }
        let program = compile(&[Op::Push(100), Op::Await, Op::Add]).unwrap();
        let (result, trace) =
            drive(AsyncExec::new(program, vec![Box::pin(core::future::ready(23))])).await;
        assert_eq!(result, 123);
        assert!(trace.is_empty(), "ready ⇒ native fast path, no park");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn two_awaits_park_in_program_order() {
        if !available() {
            return;
        }
        let program = compile(&[Op::Await, Op::Await, Op::Mul]).unwrap();
        assert_eq!(program.await_count(), 2);
        let (result, trace) =
            drive(AsyncExec::new(program, vec![later(6, 30), later(7, 60)])).await;
        assert_eq!(result, 42);
        let indices: Vec<usize> = trace.iter().map(|e| e.await_index).collect();
        assert_eq!(indices, vec![0, 1]);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn independent_awaits_resolve_concurrently() {
        if !available() {
            return;
        }
        // #0 lands LATE, #1 EARLY: by the time #0 wakes us #1 is ready, so the
        // resume sails past #1 — exactly one park.
        let program = compile(&[Op::Await, Op::Await, Op::Add]).unwrap();
        let (result, trace) =
            drive(AsyncExec::new(program, vec![later(40, 60), later(2, 20)])).await;
        assert_eq!(result, 42);
        assert_eq!(trace.len(), 1, "concurrent resolution ⇒ one park: {trace:?}");
        assert_eq!(trace[0].await_index, 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn suspension_is_inspectable_while_parked() {
        if !available() {
            return;
        }
        // push 10; push 20; add (=30); await; mul  ⇒  30 * 4 = 120.
        let program =
            compile(&[Op::Push(10), Op::Push(20), Op::Add, Op::Await, Op::Mul]).unwrap();
        let mut exec = AsyncExec::new(program, vec![later(4, 50)]);
        let mut inspected = false;
        let result = core::future::poll_fn(|cx| {
            let p = Pin::new(&mut exec).poll(cx);
            if p.is_pending()
                && let Some(s) = exec.suspension()
            {
                assert_eq!(s.await_index, 0);
                assert_eq!(s.stack, vec![30]);
                inspected = true;
            }
            p
        })
        .await;
        assert!(inspected);
        assert_eq!(result, 120);
    }
}
