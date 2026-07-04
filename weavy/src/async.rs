//! The async lane: a program that SUSPENDS at await points and RESUMES,
//! driven as a real Rust [`Future`], awaiting real Rust futures.
//!
//! This is the evaluation substrate for a demand-driven language (vix): a node
//! that awaits its inputs IS a future, the demand driver IS an executor, and a
//! cross-executor value landing over the network IS a woken waker. Coroutines
//! were the wrong shape (a parallel-stack discipline); Rust async is right
//! because the awaited things — an exec flush, a remote fetch, a producing
//! handle — are already futures.
//!
//! # Two backends, one semantics — PORTABILITY FIRST
//!
//! Evaluation must run everywhere: iOS and the browser FORBID JIT (no
//! executable pages / no native codegen), and that's exactly where a language
//! runtime often lives. So the async substrate has two execution lanes with
//! IDENTICAL semantics:
//!
//! - [`Interp`] — a pure-Rust interpreter over the op list. Always available,
//!   no `unsafe`, no executable memory. This is the REFERENCE.
//! - the JIT lane — the copy-and-patch chain, native-only (`jit` feature +
//!   supported target), for speed. It must match the interpreter exactly
//!   (differential tests assert same result AND same suspension trace).
//!
//! [`AsyncExec::new`] picks the JIT when available and falls back to the
//! interpreter otherwise; [`AsyncExec::interpret`] forces the portable lane.
//!
//! # The operand word is GENERIC
//!
//! Everything here is parameterized over [`Word`] — the operand type on the
//! stack. `i64` is the canonical instantiation (and the only one the JIT lane
//! compiles today: stencils have an ABI, so a new word grows a new stencil
//! set). vix instantiates the lane with its `Slot` word; lane selection has
//! two axes — JIT when the target supports it AND the word has stencils,
//! interpreter otherwise. A word whose arithmetic can fail (a tagged word)
//! reports [`WordFault`]s; faults surface as [`Step::Faulted`], and the vix
//! machine driver treats them as values. This wrapper's [`Future`] treats
//! them as lowering bugs (panic), matching the old strict behavior for `i64`
//! programs, which cannot fault.
//!
//! # Why suspension works in BOTH lanes
//!
//! The state that must survive a suspend is never on the C stack. In the
//! interpreter it's the machine's own fields (a program counter + a `Vec`
//! operand stack); suspend is a plain `return`. In the JIT it's an explicit
//! `Ctx` struct + the driver, and the guaranteed-tail-call discipline
//! (`become`, see `build.rs`) means the whole chain runs in one driver-owned
//! frame, so a stencil suspends by returning up — the JIT lane is the two-
//! successor type-speculation guard stencil with the slow path repurposed from
//! "deopt to interpreter" to "yield Pending to the executor".
//!
//! # The state machine (N suspend points)
//!
//! Await points are numbered in program order; the caller supplies one input
//! future per await. Readiness/values are host arrays indexed by await index,
//! and the driver polls every unresolved input each turn, so independent awaits
//! resolve CONCURRENTLY even though a lane visits them in program order. On
//! suspend a lane records WHICH await parked it (and, in the JIT, where to
//! re-enter) and returns; the driver resumes it when that input lands.
//!
//! # Debuggability (first-class, both lanes)
//!
//! [`AsyncExec::trace`] is the suspension timeline (which await parked, at what
//! operand-stack depth, in order); [`AsyncExec::suspension`] exposes the live
//! parked state (await index + operand-stack snapshot). These are backend-
//! agnostic — the same story on an iPhone (interpreted) and a build server
//! (JIT'd).

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

/// An operand word: what lives on the lane's stack. `i64` is the canonical
/// instantiation; vix instantiates with its `Slot`. Arithmetic is fallible so
/// tagged words can refuse mismatched operands ([`WordFault`]); `i64` never
/// faults.
pub trait Word: Copy + core::fmt::Debug + PartialEq + Unpin + 'static {
    fn add(self, rhs: Self) -> Result<Self, WordFault>;
    fn mul(self, rhs: Self) -> Result<Self, WordFault>;

    /// Placeholder for not-yet-ready awaited values; never observed by a
    /// correct lane (the readiness array guards every read).
    fn filler() -> Self;

    /// The word's JIT lane for this program, if this target and this word
    /// have one. Default: none — the interpreter carries the semantics.
    fn jit_lane(ops: &[Op<Self>]) -> Option<Box<dyn Machine<Self>>>
    where
        Self: Sized,
    {
        let _ = ops;
        None
    }
}

impl Word for i64 {
    fn add(self, rhs: Self) -> Result<Self, WordFault> {
        Ok(self.wrapping_add(rhs))
    }

    fn mul(self, rhs: Self) -> Result<Self, WordFault> {
        Ok(self.wrapping_mul(rhs))
    }

    fn filler() -> Self {
        0
    }

    fn jit_lane(ops: &[Op<Self>]) -> Option<Box<dyn Machine<Self>>> {
        #[cfg(feature = "jit")]
        {
            if let Some(m) = jit_lane::JitMachine::compile(ops) {
                return Some(Box::new(m));
            }
        }
        let _ = ops;
        None
    }
}

/// A word refused an operation: the program is malformed for this word (a
/// lowering bug, or an untyped program probing a tagged word). Faults are
/// deterministic and identical across lanes by construction — a faulting
/// program never reaches a JIT lane (only stencil-backed words compile, and
/// stencil-backed words don't fault).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WordFault {
    TypeMismatch { op: &'static str },
    StackUnderflow { op: &'static str },
    EmptyResult,
}

impl core::fmt::Display for WordFault {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            WordFault::TypeMismatch { op } => write!(f, "{op}: operand type mismatch"),
            WordFault::StackUnderflow { op } => write!(f, "{op}: operand stack underflow"),
            WordFault::EmptyResult => write!(f, "program finished with an empty stack"),
        }
    }
}

impl std::error::Error for WordFault {}

/// One op in an async program. `Await` points are numbered in program order;
/// the caller supplies one input per await, in the same order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op<W> {
    /// Push an immediate onto the operand stack.
    Push(W),
    /// Suspend point: await the next external input, pushing its value when
    /// ready.
    Await,
    /// Pop two, push their sum.
    Add,
    /// Pop two, push their product.
    Mul,
}

/// Number of await (suspend) points in an op list.
pub fn await_count<W>(ops: &[Op<W>]) -> usize {
    ops.iter().filter(|o| matches!(o, Op::Await)).count()
}

/// A recorded suspension — the debuggable timeline of where the chain parked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SuspendEvent {
    /// Which await point parked the chain.
    pub await_index: usize,
    /// Operand-stack depth at the moment of suspension.
    pub stack_depth: usize,
}

/// The live state of a currently-suspended chain (for inspection/debugging).
/// Backend-agnostic: identical on the interpreter and the JIT.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suspension<W> {
    /// The await point the chain is parked on.
    pub await_index: usize,
    /// Snapshot of the operand stack below the suspend point.
    pub stack: Vec<W>,
}

/// The result of running a lane from its current resume point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step<W> {
    Done(W),
    /// Parked on this await point (program order).
    Suspended(usize),
    /// The word refused an op — malformed program for this word.
    Faulted(WordFault),
}

/// An execution lane: run from the current resume point until done, suspended,
/// or faulted, over host readiness/value arrays. The lane owns its own resume
/// state (a program counter or a chain cursor) and operand stack. This is THE
/// seam a demand driver plugs into: vix's machine drives lanes directly;
/// [`AsyncExec`] wraps one lane in a `Future` over boxed inputs.
pub trait Machine<W: Word> {
    fn run(&mut self, ready: &[bool], awaited: &[W]) -> Step<W>;
    fn stack(&self) -> &[W];
}

// ---------------------------------------------------------------------------
// Interpreter lane — ALWAYS available (iOS, wasm, anything). The reference.
// ---------------------------------------------------------------------------

/// The reference lane: pure Rust, no `unsafe`, runs anywhere, defines the
/// semantics the JIT must match.
#[derive(Debug)]
pub struct Interp<W> {
    ops: Vec<Op<W>>,
    /// `await_index_at[pc]` is the await index when `ops[pc] == Await`.
    await_index_at: Vec<usize>,
    pc: usize,
    stack: Vec<W>,
}

impl<W: Word> Interp<W> {
    pub fn new(ops: &[Op<W>]) -> Self {
        let mut await_index_at = vec![0usize; ops.len()];
        let mut next = 0;
        for (pc, op) in ops.iter().enumerate() {
            if matches!(op, Op::Await) {
                await_index_at[pc] = next;
                next += 1;
            }
        }
        Interp {
            ops: ops.to_vec(),
            await_index_at,
            pc: 0,
            stack: Vec::new(),
        }
    }

    fn binary(
        &mut self,
        op_name: &'static str,
        apply: impl Fn(W, W) -> Result<W, WordFault>,
    ) -> Result<(), WordFault> {
        let b = self
            .stack
            .pop()
            .ok_or(WordFault::StackUnderflow { op: op_name })?;
        let a = self
            .stack
            .pop()
            .ok_or(WordFault::StackUnderflow { op: op_name })?;
        self.stack.push(apply(a, b)?);
        Ok(())
    }
}

impl<W: Word> Machine<W> for Interp<W> {
    fn run(&mut self, ready: &[bool], awaited: &[W]) -> Step<W> {
        loop {
            if self.pc >= self.ops.len() {
                return match self.stack.last() {
                    Some(result) => Step::Done(*result),
                    None => Step::Faulted(WordFault::EmptyResult),
                };
            }
            match self.ops[self.pc] {
                Op::Push(w) => {
                    self.stack.push(w);
                    self.pc += 1;
                }
                Op::Add => match self.binary("Add", W::add) {
                    Ok(()) => self.pc += 1,
                    Err(fault) => return Step::Faulted(fault),
                },
                Op::Mul => match self.binary("Mul", W::mul) {
                    Ok(()) => self.pc += 1,
                    Err(fault) => return Step::Faulted(fault),
                },
                Op::Await => {
                    let idx = self.await_index_at[self.pc];
                    if ready[idx] {
                        self.stack.push(awaited[idx]);
                        self.pc += 1;
                    } else {
                        // Suspend: leave pc AT the await, so resume re-checks it.
                        return Step::Suspended(idx);
                    }
                }
            }
        }
    }

    fn stack(&self) -> &[W] {
        &self.stack
    }
}

// ---------------------------------------------------------------------------
// JIT lane — native-only acceleration; i64 words only (stencils have an ABI;
// a new word grows a new stencil set). Must match the interpreter exactly.
// ---------------------------------------------------------------------------

#[cfg(feature = "jit")]
mod jit_lane {
    use super::{Machine, Op, Step};
    use crate::jit::{NativeProgram, StencilLayout, async_stencils};

    /// Threaded state — MUST match `Ctx` in `stencils/async_ops.rs`.
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

    /// Whether the JIT async lane is usable on this target.
    pub fn available() -> bool {
        !async_stencils::PUSH.is_empty() && crate::jit::NATIVE_COPY_PATCH_AVAILABLE
    }

    /// A compiled suspendable chain. `compile` bakes each await's resume offset
    /// (its own chain offset) and index into the immediate stream.
    pub struct JitMachine {
        native: NativeProgram,
        stack: Vec<i64>,
        sp_len: usize,
        prog: *const u64,
        resume_offset: usize,
        started: bool,
        suspended: i64,
        resume_scratch: u64,
        await_index_scratch: u64,
        /// The stencil ABI takes readiness as i64 words; converted per run.
        ready_scratch: Vec<i64>,
    }

    impl JitMachine {
        pub fn compile(ops: &[Op<i64>]) -> Option<JitMachine> {
            if !available() {
                return None;
            }
            let mut layout = StencilLayout::new();
            let root = layout.start_chain();
            let mut sites: Vec<(usize, &'static [usize])> = Vec::new();
            let mut await_count = 0usize;

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
                    layout.push_prog_word(root.prog_index, start as u64);
                    layout.push_prog_word(root.prog_index, await_count as u64);
                    await_count += 1;
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
            let native = NativeProgram::new(layout, root);
            let prog = native.entry_prog();
            Some(JitMachine {
                native,
                stack: vec![0; 256],
                sp_len: 0,
                prog,
                resume_offset: 0,
                started: false,
                suspended: 0,
                resume_scratch: 0,
                await_index_scratch: 0,
                ready_scratch: Vec::new(),
            })
        }
    }

    impl Machine<i64> for JitMachine {
        fn run(&mut self, ready: &[bool], awaited: &[i64]) -> Step<i64> {
            self.ready_scratch.clear();
            self.ready_scratch
                .extend(ready.iter().map(|&r| i64::from(r)));
            let entry = if self.started {
                self.resume_offset
            } else {
                self.started = true;
                0
            };
            self.suspended = 0;
            let base = self.stack.as_mut_ptr();
            let mut ctx = Ctx {
                prog: self.prog,
                sp: unsafe { base.add(self.sp_len) },
                ready: self.ready_scratch.as_ptr(),
                awaited: awaited.as_ptr(),
                resume: &mut self.resume_scratch,
                await_index: &mut self.await_index_scratch,
                suspended: &mut self.suspended,
            };
            // SAFETY: `entry` is a chain offset in this program; the copied code
            // uses the `extern "C" fn(*mut Ctx)` ABI the stencils were built with.
            let f = unsafe { self.native.chain_fn::<Ctx>(entry) };
            unsafe { f(&mut ctx) };
            self.sp_len = (ctx.sp as usize - base as usize) / size_of::<i64>();
            self.prog = ctx.prog;
            if self.suspended != 0 {
                self.resume_offset = self.resume_scratch as usize;
                Step::Suspended(self.await_index_scratch as usize)
            } else {
                Step::Done(self.stack[self.sp_len - 1])
            }
        }

        fn stack(&self) -> &[i64] {
            &self.stack[..self.sp_len]
        }
    }
}

/// Whether the JIT async lane is usable on this target (for i64 words). When
/// false, [`AsyncExec::new`] uses the interpreter (always available).
pub fn jit_available() -> bool {
    #[cfg(feature = "jit")]
    {
        jit_lane::available()
    }
    #[cfg(not(feature = "jit"))]
    {
        false
    }
}

fn best_machine<W: Word>(ops: &[Op<W>]) -> (Box<dyn Machine<W>>, bool) {
    if let Some(m) = W::jit_lane(ops) {
        return (m, true);
    }
    (Box::new(Interp::new(ops)), false)
}

/// The DRIVER: a Rust [`Future`] over a suspendable program with N awaits. One
/// input future per await; each poll drives every unresolved input, then runs
/// the lane until it completes or parks.
///
/// Depends only on `core::future` — no async runtime. Provide any executor
/// (the tests use tokio) and any input futures (in production: vox streams,
/// remote fetches). A [`Step::Faulted`] here is a panic: this wrapper serves
/// programs whose words don't fault (i64) or whose lowering guarantees typed
/// programs; a driver that treats faults as values (vix's machine) drives
/// lanes directly through [`Machine`].
pub struct AsyncExec<W: Word = i64> {
    machine: Box<dyn Machine<W>>,
    /// True if the JIT lane is in use (else the interpreter). Observable so
    /// callers/tests can confirm the portable fallback engaged.
    jit: bool,
    inners: Vec<Pin<Box<dyn Future<Output = W>>>>,
    resolved: Vec<bool>,
    ready: Vec<bool>,
    awaited: Vec<W>,
    /// Set while parked: the await index we're blocked on.
    parked_on: Option<usize>,
    /// The debuggable suspension timeline (append-only).
    pub trace: Vec<SuspendEvent>,
}

impl<W: Word> AsyncExec<W> {
    fn with_machine(
        machine: Box<dyn Machine<W>>,
        jit: bool,
        ops: &[Op<W>],
        inners: Vec<Pin<Box<dyn Future<Output = W>>>>,
    ) -> Self {
        let n = await_count(ops);
        assert_eq!(inners.len(), n, "one input future per await point");
        AsyncExec {
            machine,
            jit,
            inners,
            resolved: vec![false; n],
            ready: vec![false; n],
            awaited: vec![W::filler(); n],
            parked_on: None,
            trace: Vec::new(),
        }
    }

    /// Best available lane: JIT when this target and word support it, else
    /// the interpreter. One input future per await, in program order.
    pub fn new(ops: &[Op<W>], inners: Vec<Pin<Box<dyn Future<Output = W>>>>) -> Self {
        let (machine, jit) = best_machine(ops);
        Self::with_machine(machine, jit, ops, inners)
    }

    /// Force the portable interpreter lane (the reference; always available).
    pub fn interpret(ops: &[Op<W>], inners: Vec<Pin<Box<dyn Future<Output = W>>>>) -> Self {
        Self::with_machine(Box::new(Interp::new(ops)), false, ops, inners)
    }

    /// Whether this execution is using the JIT lane (else the interpreter).
    pub fn is_jit(&self) -> bool {
        self.jit
    }

    /// The live suspended state, if the chain is currently parked.
    pub fn suspension(&self) -> Option<Suspension<W>> {
        self.parked_on.map(|await_index| Suspension {
            await_index,
            stack: self.machine.stack().to_vec(),
        })
    }
}

impl<W: Word> Future for AsyncExec<W> {
    type Output = W;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<W> {
        let this = &mut *self;

        // Drive EVERY unresolved input; independent awaits make progress
        // concurrently, so a resume can sail past awaits that landed early.
        for i in 0..this.inners.len() {
            if !this.resolved[i]
                && let Poll::Ready(value) = this.inners[i].as_mut().poll(cx)
            {
                this.awaited[i] = value;
                this.ready[i] = true;
                this.resolved[i] = true;
            }
        }

        // Parked and the await we're BLOCKED on still isn't ready: don't
        // re-enter the lane — a wakeup from some other input can't advance this
        // suspend point. (Restores per-node waker precision that the single
        // driver otherwise loses.)
        if let Some(i) = this.parked_on
            && !this.ready[i]
        {
            return Poll::Pending;
        }

        match this.machine.run(&this.ready, &this.awaited) {
            Step::Done(result) => Poll::Ready(result),
            Step::Suspended(await_index) => {
                this.parked_on = Some(await_index);
                this.trace.push(SuspendEvent {
                    await_index,
                    stack_depth: this.machine.stack().len(),
                });
                Poll::Pending
            }
            Step::Faulted(fault) => {
                panic!("async chain faulted: {fault} (malformed program — a lowering bug)")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn later(value: i64, ms: u64) -> Pin<Box<dyn Future<Output = i64>>> {
        Box::pin(async move {
            tokio::time::sleep(Duration::from_millis(ms)).await;
            value
        })
    }

    async fn drive(mut exec: AsyncExec) -> (i64, bool, Vec<SuspendEvent>) {
        let jit = exec.is_jit();
        let result = core::future::poll_fn(|cx| Pin::new(&mut exec).poll(cx)).await;
        (result, jit, exec.trace.clone())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn interpreter_lane_is_always_available() {
        // The portability guarantee: no JIT, no unsafe, works on iOS/wasm.
        let ops = [Op::Push(40), Op::Await, Op::Add];
        let (result, jit, trace) =
            drive(AsyncExec::interpret(&ops, vec![later(2, 40)])).await;
        assert_eq!(result, 42);
        assert!(!jit, "interpret() must never use the JIT");
        assert_eq!(trace, vec![SuspendEvent { await_index: 0, stack_depth: 1 }]);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn interpreter_and_jit_agree_differentially() {
        // The JIT is an accelerator that MUST match the reference interpreter —
        // same result, same suspension trace — on every program.
        let programs: &[&[Op<i64>]] = &[
            &[Op::Push(40), Op::Await, Op::Add],
            &[Op::Await, Op::Await, Op::Mul],
            &[Op::Push(10), Op::Push(20), Op::Add, Op::Await, Op::Mul],
        ];
        for ops in programs {
            let n = await_count(ops);
            let inners_i: Vec<_> = (0..n).map(|k| later(2 + k as i64, 20)).collect();
            let inners_j: Vec<_> = (0..n).map(|k| later(2 + k as i64, 20)).collect();

            let (ri, _, ti) = drive(AsyncExec::interpret(ops, inners_i)).await;
            let exec_j = AsyncExec::new(ops, inners_j);
            if !exec_j.is_jit() {
                continue; // no JIT on this target — interpreter already checked
            }
            let (rj, _, tj) = drive(exec_j).await;
            assert_eq!(ri, rj, "result mismatch interp vs jit for {ops:?}");
            assert_eq!(ti, tj, "trace mismatch interp vs jit for {ops:?}");
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ready_input_never_suspends() {
        let ops = [Op::Push(100), Op::Await, Op::Add];
        let (result, _, trace) = drive(AsyncExec::interpret(
            &ops,
            vec![Box::pin(core::future::ready(23))],
        ))
        .await;
        assert_eq!(result, 123);
        assert!(trace.is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn independent_awaits_resolve_concurrently() {
        // #0 late, #1 early: by the time #0 wakes us #1 is ready, so the resume
        // sails past #1 — exactly one park. Checked on the interpreter (the
        // reference); the differential test proves the JIT matches.
        let ops = [Op::Await, Op::Await, Op::Add];
        let (result, _, trace) =
            drive(AsyncExec::interpret(&ops, vec![later(40, 60), later(2, 20)])).await;
        assert_eq!(result, 42);
        assert_eq!(trace.len(), 1, "concurrent ⇒ one park: {trace:?}");
        assert_eq!(trace[0].await_index, 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn suspension_is_inspectable_while_parked() {
        let ops = [Op::Push(10), Op::Push(20), Op::Add, Op::Await, Op::Mul];
        let mut exec = AsyncExec::interpret(&ops, vec![later(4, 50)]);
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

    #[test]
    fn faults_are_values_at_the_lane_level() {
        // A driver that owns its lane (vix's machine) sees faults as data.
        let ops = [Op::Push(1), Op::Add];
        let mut lane = Interp::new(&ops);
        assert_eq!(
            lane.run(&[], &[]),
            Step::Faulted(WordFault::StackUnderflow { op: "Add" })
        );
    }
}
