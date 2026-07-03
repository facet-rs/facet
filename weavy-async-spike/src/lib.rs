//! Weavy async: a JIT'd copy-and-patch chain with MULTIPLE suspend points,
//! driven as a real Rust `Future`, awaiting real Rust futures.
//!
//! This is the prerequisite for lowering vix's demand-driven evaluation to
//! weavy: a node that awaits its inputs IS a future, the demand driver IS an
//! executor, and a cross-executor part landing over vox IS a woken waker.
//!
//! # The state machine
//!
//! A compiled chain runs in ONE driver-owned stack frame. Each `await`
//! stencil either:
//!   - READY: pushes the awaited value and tail-calls the next stencil; or
//!   - PENDING: writes its own resume offset + await index into `Ctx` and
//!     RETURNS — unwinding to the driver, losing nothing (all live state is
//!     in `Ctx` / the driver, never on the C stack).
//!
//! The driver re-enters at the saved resume offset when the future lands.
//! Because readiness/values are host arrays indexed by await index, and the
//! driver polls every unresolved input each turn, independent awaits resolve
//! CONCURRENTLY even though the chain visits them in program order.
//!
//! # Debuggability
//!
//! Every suspend is recorded in a [`WeavyExec::trace`] as a [`SuspendEvent`]
//! (which await parked, at what operand-stack depth) and the live suspended
//! state is inspectable via [`WeavyExec::suspension`]. This is deliberate:
//! the production engine must be able to show WHERE a demand graph is parked
//! and on WHAT.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

mod stencils {
    include!(concat!(env!("OUT_DIR"), "/async_stencils.rs"));
}

/// MUST match `Ctx` in stencils/async_ops.rs (repr(C), same field order).
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

/// One op in the tiny async program.
#[derive(Clone, Copy, Debug)]
pub enum Op {
    Push(i64),
    /// Await the next external input (index assigned in program order). The
    /// caller supplies one future per await, in the same order.
    Await,
    Add,
    Mul,
}

/// A compiled async program plus a map from await index to the chain offset
/// where that await lives (its resume point).
pub struct AsyncProgram {
    native: weavy::jit::NativeProgram,
    /// `await_offsets[i]` = chain offset of await #i (also baked into prog as
    /// that await's resume immediate). Kept for inspection/debugging.
    await_offsets: Vec<usize>,
}

impl AsyncProgram {
    /// Number of await (suspend) points in the chain.
    pub fn await_count(&self) -> usize {
        self.await_offsets.len()
    }
}

/// Whether the async lane compiled for this target.
pub fn available() -> bool {
    !stencils::PUSH.is_empty() && weavy::jit::NATIVE_COPY_PATCH_AVAILABLE
}

/// Assemble ops into a copy-and-patch chain. Each await bakes its own chain
/// offset and index into the immediate stream (the resume machinery).
pub fn compile(ops: &[Op]) -> Option<AsyncProgram> {
    if !available() {
        return None;
    }
    use weavy::jit::StencilLayout;
    let mut layout = StencilLayout::new();
    let root = layout.start_chain();
    let mut sites: Vec<(usize, &'static [usize])> = Vec::new();
    let mut await_offsets = Vec::new();

    for op in ops {
        // Immediates are pushed in EXECUTION order (push before/after emit
        // doesn't matter; the prog stream order is what the stencils read).
        if let Op::Push(n) = op {
            layout.push_prog_word(root.prog_index, *n as u64);
        }
        let (bytes, cont): (&[u8], &'static [usize]) = match op {
            Op::Push(_) => (stencils::PUSH, stencils::PUSH_CONT),
            Op::Await => (stencils::AWAIT, stencils::AWAIT_CONT),
            Op::Add => (stencils::ADD, stencils::ADD_CONT),
            Op::Mul => (stencils::MUL, stencils::MUL_CONT),
        };
        let start = layout.emit_stencil(bytes);
        if matches!(op, Op::Await) {
            // Bake [resume_offset = own chain offset, await index].
            let index = await_offsets.len();
            layout.push_prog_word(root.prog_index, start as u64);
            layout.push_prog_word(root.prog_index, index as u64);
            await_offsets.push(start);
        }
        sites.push((start, cont));
    }

    let done = layout.emit_stencil(stencils::DONE);
    for i in 0..sites.len() {
        let (start, cont) = sites[i];
        let target = sites.get(i + 1).map(|(s, _)| *s).unwrap_or(done);
        for &rel in cont {
            layout.patch_continuation(start + rel, target);
        }
    }
    Some(AsyncProgram {
        native: weavy::jit::NativeProgram::new(layout, root),
        await_offsets,
    })
}

/// A recorded suspension — the debuggable timeline of where the chain parked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SuspendEvent {
    /// Which await point parked the chain.
    pub await_index: usize,
    /// Operand-stack depth at the moment of suspension (what's computed so far).
    pub stack_depth: usize,
}

/// The live state of a currently-suspended chain (for inspection/debugging).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suspension {
    pub await_index: usize,
    pub resume_offset: usize,
    /// Snapshot of the operand stack below the suspend point.
    pub stack: Vec<i64>,
}

/// The DRIVER: a Rust Future over a JIT'd chain with N awaits. One inner
/// future per await; each poll drives every unresolved input, then enters the
/// chain (root first, resume offset thereafter) until it completes or parks.
pub struct WeavyExec {
    program: AsyncProgram,
    inners: Vec<Pin<Box<dyn Future<Output = i64>>>>,
    resolved: Vec<bool>,
    ready: Vec<i64>,
    awaited: Vec<i64>,
    stack: Vec<i64>,
    /// Suspended cursor (persists across polls — the live state kept off the
    /// C stack, which is why it survives the unwind-to-driver).
    sp_len: usize,
    prog: *const u64,
    /// 0 before the first poll; the resume offset while suspended.
    resume_offset: usize,
    started: bool,
    // Scratch cells the stencils write on suspend.
    suspended: i64,
    resume_scratch: u64,
    await_index_scratch: u64,
    /// The debuggable suspension timeline.
    pub trace: Vec<SuspendEvent>,
}

impl WeavyExec {
    /// One future per await, in program order.
    pub fn new(
        program: AsyncProgram,
        inners: Vec<Pin<Box<dyn Future<Output = i64>>>>,
    ) -> Self {
        assert_eq!(
            inners.len(),
            program.await_count(),
            "one future per await point"
        );
        let n = inners.len();
        let prog = program.native.entry_prog();
        WeavyExec {
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
        // Persist the live cursor for the next poll.
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

impl Future for WeavyExec {
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

        // If we're parked and the await we're BLOCKED on still isn't ready,
        // don't re-enter the chain — a wakeup from some OTHER input can't let
        // this suspend point proceed. (In the real engine each node registers
        // only on its own input; here the single driver multiplexes all
        // awaits, so this guard restores that precision.)
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
