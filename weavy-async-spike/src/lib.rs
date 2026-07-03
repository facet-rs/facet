//! Weavy async crux: a JIT'd copy-and-patch chain that SUSPENDS at an await
//! point and RESUMES — driven by a real Rust `Future`, awaiting a real Rust
//! future (a oneshot fired from another task).
//!
//! This is the prerequisite proof for lowering vix's demand-driven evaluation
//! to weavy: a node that awaits its inputs IS a future, the demand driver IS
//! an executor, and a cross-executor part landing over vox IS a woken waker.
//! Coroutines were the wrong shape; Rust async is right because our awaited
//! things are already tokio futures.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

mod stencils {
    include!(concat!(env!("OUT_DIR"), "/async_stencils.rs"));
}

/// MUST match `Ctx` in stencils/async_ops.rs.
#[repr(C)]
struct Ctx {
    prog: *const u64,
    sp: *mut i64,
    ready: *const i64,
    awaited: *const i64,
    suspended: *mut i64,
}

/// One op in the tiny async program.
#[derive(Clone, Copy, Debug)]
pub enum Op {
    Push(i64),
    /// Await the (single) external value, pushing it when ready.
    Await,
    Add,
}

/// A compiled async program plus the offset of its await stencil (the resume
/// point). Real weavy would track multiple awaits by index; the crux needs one.
pub struct AsyncProgram {
    native: weavy::jit::NativeProgram,
    await_offset: Option<usize>,
}

/// Whether the async lane compiled for this target.
pub fn available() -> bool {
    !stencils::PUSH.is_empty() && weavy::jit::NATIVE_COPY_PATCH_AVAILABLE
}

/// Assemble ops into a copy-and-patch chain, recording where the await landed.
pub fn compile(ops: &[Op]) -> Option<AsyncProgram> {
    if !available() {
        return None;
    }
    use weavy::jit::StencilLayout;
    let mut layout = StencilLayout::new();
    let root = layout.start_chain();
    let mut sites: Vec<(usize, &'static [usize])> = Vec::new();
    let mut await_offset = None;
    for op in ops {
        let (bytes, cont): (&[u8], &'static [usize]) = match op {
            Op::Push(n) => {
                layout.push_prog_word(root.prog_index, *n as u64);
                (stencils::PUSH, stencils::PUSH_CONT)
            }
            Op::Await => (stencils::AWAIT, stencils::AWAIT_CONT),
            Op::Add => (stencils::ADD, stencils::ADD_CONT),
        };
        let start = layout.emit_stencil(bytes);
        if matches!(op, Op::Await) {
            await_offset = Some(start);
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
        await_offset,
    })
}

/// The DRIVER: a Rust Future over a JIT'd chain that awaits `inner`. Each poll
/// drives the real future first; if it's ready, the readiness cell is set and
/// the chain is (re-)entered — at the root on the first poll, at the await
/// offset on resume. The chain either runs to DONE (Ready) or returns having
/// set `suspended` (Pending).
pub struct WeavyExec {
    program: AsyncProgram,
    inner: Pin<Box<dyn Future<Output = i64>>>,
    stack: Vec<i64>,
    /// SUSPENDED STATE: the operand-stack depth and the immediate cursor must
    /// PERSIST across suspend/resume — they are the chain's live state, kept
    /// off the C stack precisely so it survives the unwind-to-driver. (Real
    /// weavy carries this in the Future's captured state; the crux carries it
    /// here.)
    sp_len: usize,
    prog: *const u64,
    ready: i64,
    awaited: i64,
    suspended: i64,
    started: bool,
    resolved: bool,
    /// How many times the chain actually PARKED on the await (proof the
    /// suspend path ran, not just that the arithmetic came out right).
    pub suspends: usize,
}

impl WeavyExec {
    pub fn new(program: AsyncProgram, inner: impl Future<Output = i64> + 'static) -> Self {
        let prog = program.native.entry_prog();
        WeavyExec {
            program,
            inner: Box::pin(inner),
            stack: vec![0; 64],
            sp_len: 0,
            prog,
            ready: 0,
            awaited: 0,
            suspended: 0,
            started: false,
            resolved: false,
            suspends: 0,
        }
    }

    fn run_from(&mut self, offset: usize) -> Option<i64> {
        self.suspended = 0;
        let base = self.stack.as_mut_ptr();
        let mut ctx = Ctx {
            // Resume the cursor where the last (suspended) run left it.
            prog: self.prog,
            sp: unsafe { base.add(self.sp_len) },
            ready: &self.ready,
            awaited: &self.awaited,
            suspended: &mut self.suspended,
        };
        // SAFETY: `offset` is a chain offset in this program; the copied code
        // uses the `extern "C" fn(*mut Ctx)` ABI the stencils were built with.
        let entry = unsafe { self.program.native.chain_fn::<Ctx>(offset) };
        unsafe { entry(&mut ctx) };
        // Save the cursor back — the live state that persists to the next poll.
        self.sp_len = (ctx.sp as usize - base as usize) / size_of::<i64>();
        self.prog = ctx.prog;
        if self.suspended != 0 {
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

        // Drive the REAL async input first; when it lands, arm the readiness
        // cell the await stencil reads. Its waker wakes us to re-poll.
        if !this.resolved {
            match this.inner.as_mut().poll(cx) {
                Poll::Ready(value) => {
                    this.awaited = value;
                    this.ready = 1;
                    this.resolved = true;
                }
                Poll::Pending => {}
            }
        }

        // Enter the chain: root on the first poll, the await offset on resume.
        let entry = if !this.started {
            this.started = true;
            0 // root chain offset
        } else {
            this.program
                .await_offset
                .expect("resuming a program with no await")
        };
        match this.run_from(entry) {
            Some(result) => Poll::Ready(result),
            None => {
                this.suspends += 1;
                Poll::Pending // suspended on the await; waker already set
            }
        }
    }
}
