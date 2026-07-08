//! The task lane's JIT: frame-addressed chains under copy-and-patch,
//! same semantics as [`crate::task::Task`] (the reference behavior is
//! pinned by differential tests below — results, steps, AND the
//! frame-granular trace must match).
//!
//! Division of labor (v1, per the ruled ABI): straight-line bodies run
//! NATIVE (const/add/mul/indexed loads/stores/awaits); CALL and RET
//! exit to the driver, which owns frame allocation (tested logic) and
//! trampolines into the callee's chain. Direct-threaded cross-chain
//! calls are a later optimization — the frame layout and immediate
//! vocabulary don't change when they arrive.
//!
//! Two-pass compilation: pass 1 emits every op's stencil and records
//! chain offsets (a CALL's continuation is the NEXT op's offset, known
//! only after emission); pass 2 pushes the immediate stream in op
//! order. Code and data are separate streams, so this reordering is
//! invisible at runtime.

use std::collections::HashMap;

use crate::jit::{NativeProgram, StencilLayout, task_stencils};
use crate::task::{
    Advance, ArgCopy, FnId, HostFn, Op, Program, TaskEvent, TaskStep, TraceMode, ValueMemories,
};

/// Threaded state — MUST match `Ctx` in stencils/task_ops.rs.
#[repr(C)]
struct Ctx {
    prog: *const u64,
    frame: *mut u8,
    ready: *mut i64,
    awaited: *const i64,
    resume: *mut u64,
    await_index: *mut u64,
    exit: *mut i64,
    store_value_memories: *const crate::task::ValueMemory,
    store_value_memory_count: usize,
    molten_value_memories: *const crate::task::ValueMemory,
    molten_value_memory_count: usize,
}

/// Whether the task JIT lane is usable on this target.
pub fn available() -> bool {
    !task_stencils::CONST.is_empty() && crate::jit::NATIVE_COPY_PATCH_AVAILABLE
}

#[derive(Clone, Debug)]
struct CallDesc {
    callee: FnId,
    args: Vec<ArgCopy>,
    ret: u32,
}

struct CompiledFn {
    native: NativeProgram,
    /// Call descriptors keyed by the call site's CONTINUATION chain
    /// offset (what the CALL stencil reports through `resume`).
    calls: HashMap<u64, CallDesc>,
    frame_size: usize,
    frame_align: usize,
}

/// A compiled task program: one native chain per function.
pub struct JitProgram {
    fns: Vec<CompiledFn>,
}

impl JitProgram {
    /// Compile every function (Innards instrumentation). Returns None
    /// when the lane is unavailable on this target.
    pub fn compile(program: &Program) -> Option<JitProgram> {
        Self::compile_with_mode(program, TraceMode::Innards)
    }

    /// Compile with an explicit trace mode. Production STRIPS
    /// instrumentation ops from the chains entirely — zero
    /// instructions, not skipped checks.
    pub fn compile_with_mode(program: &Program, mode: TraceMode) -> Option<JitProgram> {
        if !available() {
            return None;
        }
        let fns = program.fns.iter().map(|f| compile_fn(f, mode)).collect();
        Some(JitProgram { fns })
    }
}

/// First emitted stencil at or after op `i` (stripped ops have no
/// stencil of their own; control flows to whatever follows them).
fn next_emitted(starts: &[Option<usize>], i: usize, done: usize) -> usize {
    starts
        .iter()
        .skip(i)
        .flatten()
        .next()
        .copied()
        .unwrap_or(done)
}

fn next_emitted_prog(
    starts: &[Option<usize>],
    prog_starts: &[Option<usize>],
    i: usize,
    done: usize,
) -> usize {
    starts
        .iter()
        .enumerate()
        .skip(i)
        .find_map(|(j, start)| start.and_then(|_| prog_starts[j]))
        .unwrap_or(done)
}

fn prog_delta(from: usize, to: usize) -> u64 {
    (to as i64 - from as i64) as u64
}

#[derive(Clone, Copy)]
enum Continuations {
    Fallthrough(&'static [usize]),
    Jump(&'static [usize]),
    JumpIfZero {
        taken: &'static [usize],
        fallthrough: &'static [usize],
    },
}

fn compile_fn(f: &crate::task::Fn, mode: TraceMode) -> CompiledFn {
    let mut layout = StencilLayout::new();
    let root = layout.start_chain();

    let stripped = |op: &Op| matches!(op, Op::Trace { .. }) && mode == TraceMode::Production;

    // Pass 1: code. Record each op's chain start and continuation
    // patch sites; collect call descriptors keyed by continuation.
    // Stripped ops record no start — they own no instructions.
    let mut starts: Vec<Option<usize>> = Vec::with_capacity(f.code.len());
    let mut sites: Vec<(usize, Continuations)> = Vec::new();
    for op in &f.code {
        if stripped(op) {
            starts.push(None);
            continue;
        }
        let (bytes, cont): (&[u8], Continuations) = match op {
            Op::ConstI64 { .. } => (
                task_stencils::CONST,
                Continuations::Fallthrough(task_stencils::CONST_CONT),
            ),
            Op::AddI64 { .. } => (
                task_stencils::ADD,
                Continuations::Fallthrough(task_stencils::ADD_CONT),
            ),
            Op::MulI64 { .. } => (
                task_stencils::MUL,
                Continuations::Fallthrough(task_stencils::MUL_CONT),
            ),
            Op::SubI64 { .. } => (
                task_stencils::SUB,
                Continuations::Fallthrough(task_stencils::SUB_CONT),
            ),
            Op::CopyI64 { .. } => (
                task_stencils::COPY,
                Continuations::Fallthrough(task_stencils::COPY_CONT),
            ),
            Op::EqI64 { .. } => (
                task_stencils::EQ,
                Continuations::Fallthrough(task_stencils::EQ_CONT),
            ),
            Op::NeI64 { .. } => (
                task_stencils::NE,
                Continuations::Fallthrough(task_stencils::NE_CONT),
            ),
            Op::LtI64 { .. } => (
                task_stencils::LT,
                Continuations::Fallthrough(task_stencils::LT_CONT),
            ),
            Op::LeI64 { .. } => (
                task_stencils::LE,
                Continuations::Fallthrough(task_stencils::LE_CONT),
            ),
            Op::GtI64 { .. } => (
                task_stencils::GT,
                Continuations::Fallthrough(task_stencils::GT_CONT),
            ),
            Op::GeI64 { .. } => (
                task_stencils::GE,
                Continuations::Fallthrough(task_stencils::GE_CONT),
            ),
            Op::Jump { .. } => (
                task_stencils::JUMP,
                Continuations::Jump(task_stencils::JUMP_CONT),
            ),
            Op::JumpIfZero { .. } => (
                task_stencils::JUMP_IF_ZERO,
                Continuations::JumpIfZero {
                    taken: task_stencils::JUMP_IF_ZERO_TAKEN_CONT,
                    fallthrough: task_stencils::JUMP_IF_ZERO_FALLTHROUGH_CONT,
                },
            ),
            Op::LoadIndexedI64 { .. } => (
                task_stencils::LOAD_IX,
                Continuations::Fallthrough(task_stencils::LOAD_IX_CONT),
            ),
            Op::StoreIndexedI64 { .. } => (
                task_stencils::STORE_IX,
                Continuations::Fallthrough(task_stencils::STORE_IX_CONT),
            ),
            Op::LoadArrayWord { .. } => (
                task_stencils::LOAD_ARRAY_WORD,
                Continuations::Fallthrough(task_stencils::LOAD_ARRAY_WORD_CONT),
            ),
            Op::Await { .. } => (
                task_stencils::AWAIT,
                Continuations::Fallthrough(task_stencils::AWAIT_CONT),
            ),
            Op::Call { .. } => (
                task_stencils::CALL,
                Continuations::Fallthrough(task_stencils::CALL_CONT),
            ),
            Op::Ret { .. } => (
                task_stencils::RET,
                Continuations::Fallthrough(task_stencils::RET_CONT),
            ),
            Op::HostCall { .. } => (
                task_stencils::HOSTCALL,
                Continuations::Fallthrough(task_stencils::HOSTCALL_CONT),
            ),
            Op::HostCallYield { .. } => (
                task_stencils::HOSTCALL_YIELD,
                Continuations::Fallthrough(task_stencils::HOSTCALL_YIELD_CONT),
            ),
            // A 64-bit immediate store is type-blind: ConstF64 IS the
            // CONST stencil with float bits in the immediate.
            Op::ConstF64 { .. } => (
                task_stencils::CONST,
                Continuations::Fallthrough(task_stencils::CONST_CONT),
            ),
            Op::AddF64 { .. } => (
                task_stencils::ADD_F64,
                Continuations::Fallthrough(task_stencils::ADD_F64_CONT),
            ),
            Op::MulF64 { .. } => (
                task_stencils::MUL_F64,
                Continuations::Fallthrough(task_stencils::MUL_F64_CONT),
            ),
            Op::Trace { .. } => (
                task_stencils::TRACE,
                Continuations::Fallthrough(task_stencils::TRACE_CONT),
            ),
        };
        let start = layout.emit_stencil(bytes);
        starts.push(Some(start));
        sites.push((start, cont));
    }
    let done = layout.emit_stencil(task_stencils::DONE);
    // Continuations flow to the next EMITTED stencil, skipping
    // stripped ops (they own no code to flow through).
    let mut emitted_ix = 0usize;
    for (i, start_opt) in starts.iter().enumerate() {
        let Some(start) = *start_opt else { continue };
        let (_, cont) = sites[emitted_ix];
        emitted_ix += 1;
        match cont {
            Continuations::Fallthrough(relocs) => {
                let target = next_emitted(&starts, i + 1, done);
                for &rel in relocs {
                    layout.patch_continuation(start + rel, target);
                }
            }
            Continuations::Jump(relocs) => {
                let Op::Jump { target } = &f.code[i] else {
                    unreachable!("jump continuation kind only assigned to Jump")
                };
                let target = next_emitted(&starts, *target as usize, done);
                for &rel in relocs {
                    layout.patch_continuation(start + rel, target);
                }
            }
            Continuations::JumpIfZero { taken, fallthrough } => {
                let Op::JumpIfZero { target, .. } = &f.code[i] else {
                    unreachable!("conditional continuation kind only assigned to JumpIfZero")
                };
                let taken_target = next_emitted(&starts, *target as usize, done);
                let fallthrough_target = next_emitted(&starts, i + 1, done);
                for &rel in taken {
                    layout.patch_continuation(start + rel, taken_target);
                }
                for &rel in fallthrough {
                    layout.patch_continuation(start + rel, fallthrough_target);
                }
            }
        }
    }

    let mut prog_starts = Vec::with_capacity(f.code.len());
    let mut prog_len = 0usize;
    for op in &f.code {
        if stripped(op) {
            prog_starts.push(None);
            continue;
        }
        prog_starts.push(Some(prog_len));
        prog_len += match op {
            Op::ConstI64 { .. } | Op::ConstF64 { .. } => 2,
            Op::CopyI64 { .. } => 2,
            Op::AddI64 { .. }
            | Op::MulI64 { .. }
            | Op::SubI64 { .. }
            | Op::EqI64 { .. }
            | Op::NeI64 { .. }
            | Op::LtI64 { .. }
            | Op::LeI64 { .. }
            | Op::GtI64 { .. }
            | Op::GeI64 { .. }
            | Op::AddF64 { .. }
            | Op::MulF64 { .. } => 3,
            Op::Jump { .. } => 1,
            Op::JumpIfZero { .. } => 3,
            Op::LoadIndexedI64 { .. } | Op::StoreIndexedI64 { .. } => 4,
            Op::LoadArrayWord { .. } => 5,
            Op::Await { .. } => 3,
            Op::Call { .. } => 1,
            Op::Ret { .. } => 2,
            Op::HostCall { .. } | Op::HostCallYield { .. } | Op::Trace { .. } => 2,
        };
    }

    // Pass 2: the immediate stream, in op order (consumption order).
    let mut calls = HashMap::new();
    for (i, op) in f.code.iter().enumerate() {
        match op {
            Op::ConstI64 { dst, value } => {
                layout.push_prog_word(root.prog_index, u64::from(*dst));
                layout.push_prog_word(root.prog_index, *value as u64);
            }
            Op::CopyI64 { dst, src } => {
                layout.push_prog_word(root.prog_index, u64::from(*dst));
                layout.push_prog_word(root.prog_index, u64::from(*src));
            }
            Op::AddI64 { dst, a, b }
            | Op::MulI64 { dst, a, b }
            | Op::SubI64 { dst, a, b }
            | Op::EqI64 { dst, a, b }
            | Op::NeI64 { dst, a, b }
            | Op::LtI64 { dst, a, b }
            | Op::LeI64 { dst, a, b }
            | Op::GtI64 { dst, a, b }
            | Op::GeI64 { dst, a, b } => {
                layout.push_prog_word(root.prog_index, u64::from(*dst));
                layout.push_prog_word(root.prog_index, u64::from(*a));
                layout.push_prog_word(root.prog_index, u64::from(*b));
            }
            Op::Jump { target } => {
                let here = prog_starts[i].expect("jumps are never stripped");
                let target = next_emitted_prog(&starts, &prog_starts, *target as usize, prog_len);
                layout.push_prog_word(root.prog_index, prog_delta(here, target));
            }
            Op::JumpIfZero { value, target } => {
                let here = prog_starts[i].expect("branches are never stripped");
                let taken = next_emitted_prog(&starts, &prog_starts, *target as usize, prog_len);
                let fallthrough = next_emitted_prog(&starts, &prog_starts, i + 1, prog_len);
                layout.push_prog_word(root.prog_index, u64::from(*value));
                layout.push_prog_word(root.prog_index, prog_delta(here, taken));
                layout.push_prog_word(root.prog_index, prog_delta(here, fallthrough));
            }
            Op::LoadIndexedI64 {
                dst,
                base,
                index,
                stride,
            } => {
                for v in [dst, base, index, stride] {
                    layout.push_prog_word(root.prog_index, u64::from(*v));
                }
            }
            Op::StoreIndexedI64 {
                base,
                index,
                stride,
                src,
            } => {
                for v in [base, index, stride, src] {
                    layout.push_prog_word(root.prog_index, u64::from(*v));
                }
            }
            Op::LoadArrayWord {
                dst,
                present,
                array,
                index,
                elem_schema_ref,
            } => {
                for v in [
                    u64::from(*dst),
                    u64::from(*present),
                    u64::from(*array),
                    u64::from(*index),
                    *elem_schema_ref as u64,
                ] {
                    layout.push_prog_word(root.prog_index, v);
                }
            }
            Op::Await { dst, input } => {
                // [resume_off = own start, index, dst] — idempotent
                // suspend point, the proven protocol. Awaits are never
                // stripped.
                layout.push_prog_word(
                    root.prog_index,
                    starts[i].expect("awaits are never stripped") as u64,
                );
                layout.push_prog_word(root.prog_index, u64::from(*input));
                layout.push_prog_word(root.prog_index, u64::from(*dst));
            }
            Op::Call { callee, args, ret } => {
                // [continuation = next emitted stencil]; descriptor
                // lives in the side table under that same key.
                let continuation = next_emitted(&starts, i + 1, done) as u64;
                layout.push_prog_word(root.prog_index, continuation);
                calls.insert(
                    continuation,
                    CallDesc {
                        callee: *callee,
                        args: args.clone(),
                        ret: *ret,
                    },
                );
            }
            Op::Ret { src, size } => {
                layout.push_prog_word(root.prog_index, u64::from(*src));
                layout.push_prog_word(root.prog_index, u64::from(*size));
            }
            Op::HostCall { host } => {
                let continuation = next_emitted(&starts, i + 1, done) as u64;
                layout.push_prog_word(root.prog_index, continuation);
                layout.push_prog_word(root.prog_index, u64::from(*host));
            }
            Op::HostCallYield { host } => {
                let continuation = next_emitted(&starts, i + 1, done) as u64;
                layout.push_prog_word(root.prog_index, continuation);
                layout.push_prog_word(root.prog_index, u64::from(*host));
            }
            Op::Trace { id } => {
                if !stripped(op) {
                    let continuation = next_emitted(&starts, i + 1, done) as u64;
                    layout.push_prog_word(root.prog_index, continuation);
                    layout.push_prog_word(root.prog_index, u64::from(*id));
                }
            }
            Op::ConstF64 { dst, bits } => {
                layout.push_prog_word(root.prog_index, u64::from(*dst));
                layout.push_prog_word(root.prog_index, *bits);
            }
            Op::AddF64 { dst, a, b } | Op::MulF64 { dst, a, b } => {
                layout.push_prog_word(root.prog_index, u64::from(*dst));
                layout.push_prog_word(root.prog_index, u64::from(*a));
                layout.push_prog_word(root.prog_index, u64::from(*b));
            }
        }
    }

    let native = NativeProgram::new(layout, root);
    CompiledFn {
        native,
        calls,
        frame_size: f.frame.size,
        frame_align: f.frame.align,
    }
}

#[derive(Clone, Debug)]
struct JitFrame {
    fn_id: FnId,
    /// Arena offset of this frame's first byte.
    base: usize,
    /// Chain offset to (re-)enter.
    resume: usize,
    /// Immediate-stream position (word index from the entry prog).
    prog_pos: usize,
    /// Absolute arena offset where our Ret writes; None for the root.
    ret_to: Option<usize>,
}

/// The JIT task driver: same observable behavior as
/// [`crate::task::Task`], frames in the same arena discipline.
pub struct JitTask {
    arena: Vec<u8>,
    frames: Vec<JitFrame>,
    pub result: Vec<u8>,
    pub trace: Vec<TaskEvent>,
    parked_on: Option<u32>,
    ready_scratch: Vec<i64>,
}

impl JitTask {
    pub fn spawn(program: &JitProgram, entry: FnId) -> Self {
        let mut task = JitTask {
            arena: Vec::new(),
            frames: Vec::new(),
            result: Vec::new(),
            trace: Vec::new(),
            parked_on: None,
            ready_scratch: Vec::new(),
        };
        let base = task.alloc_frame(&program.fns[entry.0 as usize]);
        task.frames.push(JitFrame {
            fn_id: entry,
            base,
            resume: 0,
            prog_pos: 0,
            ret_to: None,
        });
        task.trace.push(TaskEvent::FrameEntered(entry));
        task
    }

    pub fn depth(&self) -> usize {
        self.frames.len()
    }

    pub fn result_i64(&self) -> i64 {
        i64::from_le_bytes(self.result[..8].try_into().expect("8-byte result"))
    }

    /// Write an i64 into the CURRENT frame at `offset` — used for
    /// entry arguments before the first [`JitTask::run`], matching
    /// [`Task::write_i64`](crate::task::Task::write_i64).
    pub fn write_i64(&mut self, offset: u32, value: i64) {
        let base = self.frames.last().expect("live frame").base;
        let at = base + offset as usize;
        self.arena[at..at + 8].copy_from_slice(&value.to_le_bytes());
    }

    fn alloc_frame(&mut self, f: &CompiledFn) -> usize {
        let align = f.frame_align.max(1);
        let base = self.arena.len().div_ceil(align) * align;
        self.arena.resize(base + f.frame_size, 0);
        base
    }

    pub fn run(&mut self, program: &JitProgram, ready: &mut [bool], awaited: &[i64]) -> TaskStep {
        self.run_hosted(program, ready, awaited, &mut [])
    }

    pub fn run_hosted(
        &mut self,
        program: &JitProgram,
        ready: &mut [bool],
        awaited: &[i64],
        hosts: &mut [HostFn<'_>],
    ) -> TaskStep {
        self.run_hosted_with_value_memories(program, ready, awaited, hosts, ValueMemories::empty())
    }

    pub fn run_hosted_with_value_memories(
        &mut self,
        program: &JitProgram,
        ready: &mut [bool],
        awaited: &[i64],
        hosts: &mut [HostFn<'_>],
        value_memories: ValueMemories<'_>,
    ) -> TaskStep {
        self.ready_scratch.clear();
        self.ready_scratch
            .extend(ready.iter().map(|&r| i64::from(r)));
        if let Some(input) = self.parked_on
            && ready.get(input as usize).copied().unwrap_or(false)
        {
            self.parked_on = None;
            self.trace.push(TaskEvent::Resumed);
        }
        loop {
            let frame = self
                .frames
                .last()
                .expect("running task has a frame")
                .clone();
            let compiled = &program.fns[frame.fn_id.0 as usize];

            let entry_prog = compiled.native.entry_prog();
            let mut resume_scratch = 0u64;
            let mut index_scratch = 0u64;
            let mut exit_scratch = 0i64;
            let arena_base = self.arena.as_mut_ptr();
            let mut ctx = Ctx {
                prog: unsafe { entry_prog.add(frame.prog_pos) },
                frame: unsafe { arena_base.add(frame.base) },
                ready: self.ready_scratch.as_mut_ptr(),
                awaited: awaited.as_ptr(),
                resume: &mut resume_scratch,
                await_index: &mut index_scratch,
                exit: &mut exit_scratch,
                store_value_memories: value_memories.store.as_ptr(),
                store_value_memory_count: value_memories.store.len(),
                molten_value_memories: value_memories.molten.as_ptr(),
                molten_value_memory_count: value_memories.molten.len(),
            };
            // SAFETY: `frame.resume` is a chain offset of this compiled
            // function; the copied code uses the extern "C" fn(*mut Ctx)
            // ABI its stencils were built with. No arena allocation
            // happens while the chain runs (driver-only allocation).
            let f = unsafe { compiled.native.chain_fn::<Ctx>(frame.resume) };
            unsafe { f(&mut ctx) };
            for (dst, &src) in ready.iter_mut().zip(&self.ready_scratch) {
                *dst = src != 0;
            }
            let new_prog_pos = (ctx.prog as usize - entry_prog as usize) / size_of::<u64>();
            {
                let top = self.frames.last_mut().expect("frame");
                top.prog_pos = new_prog_pos;
            }
            match exit_scratch {
                1 => {
                    // Parked on an await; re-enter the await itself.
                    let input = u32::try_from(index_scratch).expect("input fits u32");
                    let top = self.frames.last_mut().expect("frame");
                    top.resume = usize::try_from(resume_scratch).expect("offset");
                    if self.parked_on != Some(input) {
                        self.parked_on = Some(input);
                        self.trace.push(TaskEvent::Parked { input });
                    }
                    return TaskStep::Parked { input };
                }
                2 => {
                    // Call: continuation offset doubles as the side-table key.
                    let continuation = resume_scratch;
                    let desc = compiled.calls[&continuation].clone();
                    {
                        let top = self.frames.last_mut().expect("frame");
                        top.resume = usize::try_from(continuation).expect("offset");
                    }
                    let callee = &program.fns[desc.callee.0 as usize];
                    let callee_base = self.alloc_frame(callee);
                    for copy in &desc.args {
                        let src = frame.base + copy.src as usize;
                        let dst = callee_base + copy.dst as usize;
                        self.arena.copy_within(src..src + copy.size as usize, dst);
                    }
                    self.frames.push(JitFrame {
                        fn_id: desc.callee,
                        base: callee_base,
                        resume: 0,
                        prog_pos: 0,
                        ret_to: Some(frame.base + desc.ret as usize),
                    });
                    self.trace.push(TaskEvent::FrameEntered(desc.callee));
                }
                3 => {
                    let src = frame.base + usize::try_from(resume_scratch).expect("src");
                    let size = usize::try_from(index_scratch).expect("size");
                    let popped = self.frames.pop().expect("frame to return from");
                    self.trace.push(TaskEvent::FrameExited(popped.fn_id));
                    match popped.ret_to {
                        Some(ret_to) => {
                            self.arena.copy_within(src..src + size, ret_to);
                        }
                        None => {
                            self.result = self.arena[src..src + size].to_vec();
                            return TaskStep::Done;
                        }
                    }
                }
                5 => {
                    // Instrumentation mark (Innards-compiled chains only).
                    let continuation = usize::try_from(resume_scratch).expect("offset");
                    let id = u32::try_from(index_scratch).expect("mark id");
                    let top = self.frames.last_mut().expect("frame");
                    top.resume = continuation;
                    self.trace.push(TaskEvent::Mark(id));
                }
                4 => {
                    // Sync host call: invoke over the frame bytes,
                    // re-enter at the continuation. No trace event, no
                    // park path — the ruled sync/async distinction.
                    let continuation = usize::try_from(resume_scratch).expect("offset");
                    let host = usize::try_from(index_scratch).expect("host index");
                    {
                        let top = self.frames.last_mut().expect("frame");
                        top.resume = continuation;
                    }
                    let end = frame.base + compiled.frame_size;
                    hosts[host](&mut self.arena[frame.base..end]);
                }
                6 => {
                    let continuation = usize::try_from(resume_scratch).expect("offset");
                    let host = usize::try_from(index_scratch).expect("host index");
                    {
                        let top = self.frames.last_mut().expect("frame");
                        top.resume = continuation;
                    }
                    let end = frame.base + compiled.frame_size;
                    hosts[host](&mut self.arena[frame.base..end]);
                    return TaskStep::Yielded;
                }
                code => panic!("task chain exited with code {code} (fell through without Ret?)"),
            }
        }
    }
}

/// The JIT lane bundled with its compiled program, for
/// [`crate::task::TaskExec`].
pub struct JitRunning<'p> {
    pub program: &'p JitProgram,
    pub task: JitTask,
}

impl Advance for JitRunning<'_> {
    fn advance(
        &mut self,
        ready: &mut [bool],
        awaited: &[i64],
        hosts: &mut [HostFn<'_>],
        value_memories: ValueMemories<'_>,
    ) -> TaskStep {
        self.task.run_hosted_with_value_memories(
            self.program,
            ready,
            awaited,
            hosts,
            value_memories,
        )
    }

    fn result_bytes(&self) -> &[u8] {
        &self.task.result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mem::Layout;
    use crate::task::{Fn as TaskFn, Task};

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn jit_tasks_await_real_futures() {
        let program = Program {
            fns: vec![TaskFn {
                frame: Layout { size: 24, align: 8 },
                code: vec![
                    Op::Await { dst: 0, input: 0 },
                    Op::Await { dst: 8, input: 1 },
                    Op::AddI64 {
                        dst: 16,
                        a: 0,
                        b: 8,
                    },
                    Op::Ret { src: 16, size: 8 },
                ],
            }],
        };
        let Some(jit) = JitProgram::compile(&program) else {
            return;
        };
        let running = JitRunning {
            program: &jit,
            task: JitTask::spawn(&jit, FnId(0)),
        };
        let slow: core::pin::Pin<Box<dyn core::future::Future<Output = i64>>> = Box::pin(async {
            tokio::time::sleep(std::time::Duration::from_millis(40)).await;
            40
        });
        let fast: core::pin::Pin<Box<dyn core::future::Future<Output = i64>>> =
            Box::pin(async { 2 });
        let result = crate::task::TaskExec::new(running, vec![slow, fast], vec![]).await;
        assert_eq!(i64::from_le_bytes(result[..8].try_into().unwrap()), 42);
    }

    #[test]
    fn trace_marks_record_in_innards_and_vanish_in_production() {
        let program = Program {
            fns: vec![
                TaskFn {
                    frame: Layout { size: 16, align: 8 },
                    code: vec![
                        Op::Trace { id: 10 },
                        Op::ConstI64 { dst: 0, value: 5 },
                        Op::Call {
                            callee: FnId(1),
                            args: vec![ArgCopy {
                                src: 0,
                                dst: 0,
                                size: 8,
                            }],
                            ret: 8,
                        },
                        Op::Trace { id: 11 },
                        Op::Ret { src: 8, size: 8 },
                    ],
                },
                TaskFn {
                    frame: Layout { size: 16, align: 8 },
                    code: vec![
                        Op::Trace { id: 20 },
                        Op::AddI64 { dst: 8, a: 0, b: 0 },
                        Op::Ret { src: 8, size: 8 },
                    ],
                },
            ],
        };

        // Innards: marks appear, in program order, in BOTH lanes.
        let mut interp = Task::spawn(&program, FnId(0));
        assert_eq!(interp.run(&program, &mut [], &[]), TaskStep::Done);
        assert_eq!(interp.result_i64(), 10);
        let marks: Vec<_> = interp
            .trace
            .iter()
            .filter_map(|e| match e {
                TaskEvent::Mark(id) => Some(*id),
                _ => None,
            })
            .collect();
        assert_eq!(marks, vec![10, 20, 11]);

        // Production: marks are GONE, everything else identical.
        let mut prod = Task::spawn_with_mode(&program, FnId(0), TraceMode::Production);
        assert_eq!(prod.run(&program, &mut [], &[]), TaskStep::Done);
        assert_eq!(prod.result_i64(), 10);
        assert!(!prod.trace.iter().any(|e| matches!(e, TaskEvent::Mark(_))));
        let stripped_of_marks: Vec<_> = interp
            .trace
            .iter()
            .copied()
            .filter(|e| !matches!(e, TaskEvent::Mark(_)))
            .collect();
        assert_eq!(prod.trace, stripped_of_marks);

        // JIT, both modes, matching the interpreter exactly.
        if let Some(jit) = JitProgram::compile(&program) {
            let mut t = JitTask::spawn(&jit, FnId(0));
            assert_eq!(t.run(&jit, &mut [], &[]), TaskStep::Done);
            assert_eq!(t.result_i64(), 10);
            assert_eq!(t.trace, interp.trace);
        }
        if let Some(jit) = JitProgram::compile_with_mode(&program, TraceMode::Production) {
            let mut t = JitTask::spawn(&jit, FnId(0));
            assert_eq!(t.run(&jit, &mut [], &[]), TaskStep::Done);
            assert_eq!(t.result_i64(), 10);
            assert_eq!(t.trace, prod.trace);
        }
    }

    #[test]
    fn f64_arithmetic_matches_the_interpreter_bitwise() {
        // (2.5 * awaited) + 0.125, parked mid-flight with float state
        // frame-resident. Same hardware, same IEEE ops in both lanes:
        // results must match BITWISE.
        let program = Program {
            fns: vec![TaskFn {
                frame: Layout { size: 32, align: 8 },
                code: vec![
                    Op::ConstF64 {
                        dst: 0,
                        bits: 2.5f64.to_bits(),
                    },
                    Op::Await { dst: 8, input: 0 },
                    Op::MulF64 {
                        dst: 16,
                        a: 0,
                        b: 8,
                    },
                    Op::ConstF64 {
                        dst: 24,
                        bits: 0.125f64.to_bits(),
                    },
                    Op::AddF64 {
                        dst: 16,
                        a: 16,
                        b: 24,
                    },
                    Op::Ret { src: 16, size: 8 },
                ],
            }],
        };
        let awaited_bits = 3.25f64.to_bits() as i64;
        differential(
            &program,
            FnId(0),
            &[(&[false], &[0]), (&[true], &[awaited_bits])],
        );
        // And the value itself is what IEEE says.
        let mut interp = Task::spawn(&program, FnId(0));
        let mut ready = [true];
        interp.run(&program, &mut ready, &[awaited_bits]);
        let bits = interp.result_i64() as u64;
        assert_eq!(f64::from_bits(bits), 2.5 * 3.25 + 0.125);
    }

    #[test]
    fn sync_host_calls_match_the_interpreter_and_never_park() {
        // host 0: read slot 0, write slot0*2+1 to slot 8. Counters
        // prove exactly-once invocation per lane.
        let program = Program {
            fns: vec![TaskFn {
                frame: Layout { size: 16, align: 8 },
                code: vec![
                    Op::ConstI64 { dst: 0, value: 20 },
                    Op::HostCall { host: 0 },
                    Op::AddI64 { dst: 8, a: 8, b: 0 },
                    Op::Ret { src: 8, size: 8 },
                ],
            }],
        };
        let host_impl = |frame: &mut [u8]| {
            let v = i64::from_le_bytes(frame[0..8].try_into().unwrap());
            frame[8..16].copy_from_slice(&(v * 2 + 1).to_le_bytes());
        };

        let mut interp_calls = 0u32;
        let mut interp_host = |frame: &mut [u8]| {
            interp_calls += 1;
            host_impl(frame);
        };
        let mut interp = Task::spawn(&program, FnId(0));
        assert_eq!(
            interp.run_hosted(&program, &mut [], &[], &mut [&mut interp_host]),
            TaskStep::Done
        );
        assert_eq!(interp.result_i64(), 61);
        assert_eq!(interp_calls, 1);
        assert!(
            !interp
                .trace
                .iter()
                .any(|e| matches!(e, TaskEvent::Parked { .. }))
        );

        let Some(jit) = JitProgram::compile(&program) else {
            return;
        };
        let mut jit_calls = 0u32;
        let mut jit_host = |frame: &mut [u8]| {
            jit_calls += 1;
            host_impl(frame);
        };
        let mut task = JitTask::spawn(&jit, FnId(0));
        assert_eq!(
            task.run_hosted(&jit, &mut [], &[], &mut [&mut jit_host]),
            TaskStep::Done
        );
        assert_eq!(task.result_i64(), 61);
        assert_eq!(jit_calls, 1);
        assert_eq!(task.trace, interp.trace);
    }

    /// Drive interp and JIT through the same schedule; assert identical
    /// steps, results, and traces.
    fn differential(program: &Program, entry: FnId, schedule: &[(&[bool], &[i64])]) {
        differential_with_mode(program, entry, schedule, TraceMode::Innards);
    }

    fn differential_with_mode(
        program: &Program,
        entry: FnId,
        schedule: &[(&[bool], &[i64])],
        mode: TraceMode,
    ) {
        let mut interp = Task::spawn_with_mode(program, entry, mode);
        let mut interp_steps = Vec::new();
        for (ready, awaited) in schedule {
            let mut ready = ready.to_vec();
            let step = interp.run(program, &mut ready, awaited);
            interp_steps.push(step);
            if step == TaskStep::Done {
                break;
            }
        }

        let Some(jit) = JitProgram::compile_with_mode(program, mode) else {
            assert!(
                !available(),
                "task JIT refused a program on a native copy-and-patch target"
            );
            return;
        };
        let mut task = JitTask::spawn(&jit, entry);
        let mut jit_steps = Vec::new();
        for (ready, awaited) in schedule {
            let mut ready = ready.to_vec();
            let step = task.run(&jit, &mut ready, awaited);
            jit_steps.push(step);
            if step == TaskStep::Done {
                break;
            }
        }

        assert_eq!(jit_steps, interp_steps, "step sequences diverge");
        assert_eq!(task.result, interp.result, "results diverge");
        assert_eq!(task.trace, interp.trace, "frame traces diverge");
    }

    #[test]
    fn seeded_root_args_match_the_interpreter_and_feed_branches() {
        let program = Program {
            fns: vec![TaskFn {
                frame: frame_of_i64s(5),
                code: vec![
                    Op::GtI64 {
                        dst: 24,
                        a: 0,
                        b: 8,
                    },
                    Op::JumpIfZero {
                        value: 24,
                        target: 5,
                    },
                    Op::SubI64 {
                        dst: 32,
                        a: 0,
                        b: 8,
                    },
                    Op::MulI64 {
                        dst: 32,
                        a: 32,
                        b: 16,
                    },
                    Op::Jump { target: 7 },
                    Op::SubI64 {
                        dst: 32,
                        a: 8,
                        b: 0,
                    },
                    Op::MulI64 {
                        dst: 32,
                        a: 32,
                        b: 16,
                    },
                    Op::Ret { src: 32, size: 8 },
                ],
            }],
        };

        let mut interp = Task::spawn(&program, FnId(0));
        interp.write_i64(0, 11);
        interp.write_i64(8, 4);
        interp.write_i64(16, 3);
        let interp_steps = vec![interp.run(&program, &mut [], &[])];

        let Some(jit) = JitProgram::compile(&program) else {
            assert!(
                !available(),
                "task JIT refused a seeded-args program on a native copy-and-patch target"
            );
            return;
        };
        let mut task = JitTask::spawn(&jit, FnId(0));
        task.write_i64(0, 11);
        task.write_i64(8, 4);
        task.write_i64(16, 3);
        let jit_steps = vec![task.run(&jit, &mut [], &[])];

        assert_eq!(jit_steps.len(), interp_steps.len(), "step counts diverge");
        assert_eq!(jit_steps, interp_steps, "step sequences diverge");
        assert_eq!(task.result, interp.result, "results diverge");
        assert_eq!(task.result_i64(), 21);
        assert_eq!(task.trace, interp.trace, "frame traces diverge");
    }

    #[test]
    fn i64_comparisons_match_the_interpreter() {
        let program = Program {
            fns: vec![TaskFn {
                frame: frame_of_i64s(10),
                code: vec![
                    Op::ConstI64 { dst: 0, value: 3 },
                    Op::ConstI64 { dst: 8, value: 5 },
                    Op::ConstI64 { dst: 16, value: 5 },
                    Op::EqI64 {
                        dst: 24,
                        a: 8,
                        b: 16,
                    },
                    Op::NeI64 {
                        dst: 32,
                        a: 0,
                        b: 8,
                    },
                    Op::LtI64 {
                        dst: 40,
                        a: 0,
                        b: 8,
                    },
                    Op::LeI64 {
                        dst: 48,
                        a: 8,
                        b: 16,
                    },
                    Op::GtI64 {
                        dst: 56,
                        a: 8,
                        b: 0,
                    },
                    Op::GeI64 {
                        dst: 64,
                        a: 8,
                        b: 16,
                    },
                    Op::AddI64 {
                        dst: 72,
                        a: 24,
                        b: 32,
                    },
                    Op::AddI64 {
                        dst: 72,
                        a: 72,
                        b: 40,
                    },
                    Op::AddI64 {
                        dst: 72,
                        a: 72,
                        b: 48,
                    },
                    Op::AddI64 {
                        dst: 72,
                        a: 72,
                        b: 56,
                    },
                    Op::AddI64 {
                        dst: 72,
                        a: 72,
                        b: 64,
                    },
                    Op::Ret { src: 72, size: 8 },
                ],
            }],
        };
        differential(&program, FnId(0), &[(&[], &[])]);

        let mut interp = Task::spawn(&program, FnId(0));
        assert_eq!(interp.run(&program, &mut [], &[]), TaskStep::Done);
        assert_eq!(interp.result_i64(), 6);
    }

    #[test]
    fn forward_jump_matches_the_interpreter() {
        let program = Program {
            fns: vec![TaskFn {
                frame: frame_of_i64s(1),
                code: vec![
                    Op::ConstI64 { dst: 0, value: 1 },
                    Op::Jump { target: 3 },
                    Op::ConstI64 { dst: 0, value: 99 },
                    Op::ConstI64 { dst: 0, value: 41 },
                    Op::Ret { src: 0, size: 8 },
                ],
            }],
        };
        differential(&program, FnId(0), &[(&[], &[])]);
    }

    #[test]
    fn backward_jump_loop_matches_the_interpreter() {
        let program = Program {
            fns: vec![TaskFn {
                frame: frame_of_i64s(5),
                code: vec![
                    Op::ConstI64 { dst: 0, value: 5 },
                    Op::ConstI64 { dst: 8, value: 0 },
                    Op::ConstI64 { dst: 16, value: 0 },
                    Op::ConstI64 { dst: 24, value: 1 },
                    Op::EqI64 {
                        dst: 32,
                        a: 0,
                        b: 16,
                    },
                    Op::JumpIfZero {
                        value: 32,
                        target: 7,
                    },
                    Op::Ret { src: 8, size: 8 },
                    Op::AddI64 { dst: 8, a: 8, b: 0 },
                    Op::SubI64 {
                        dst: 0,
                        a: 0,
                        b: 24,
                    },
                    Op::Jump { target: 4 },
                ],
            }],
        };
        differential(&program, FnId(0), &[(&[], &[])]);

        let mut interp = Task::spawn(&program, FnId(0));
        assert_eq!(interp.run(&program, &mut [], &[]), TaskStep::Done);
        assert_eq!(interp.result_i64(), 15);
    }

    #[test]
    fn jump_if_zero_taken_and_not_taken_match_the_interpreter() {
        let program = Program {
            fns: vec![TaskFn {
                frame: frame_of_i64s(3),
                code: vec![
                    Op::ConstI64 { dst: 0, value: 0 },
                    Op::JumpIfZero {
                        value: 0,
                        target: 4,
                    },
                    Op::ConstI64 { dst: 8, value: 99 },
                    Op::Jump { target: 5 },
                    Op::ConstI64 { dst: 8, value: 10 },
                    Op::ConstI64 { dst: 0, value: 1 },
                    Op::JumpIfZero {
                        value: 0,
                        target: 9,
                    },
                    Op::ConstI64 { dst: 16, value: 5 },
                    Op::AddI64 {
                        dst: 8,
                        a: 8,
                        b: 16,
                    },
                    Op::Ret { src: 8, size: 8 },
                ],
            }],
        };
        differential(&program, FnId(0), &[(&[], &[])]);
    }

    #[test]
    fn match_shaped_eq_jump_chain_matches_the_interpreter() {
        let program = Program {
            fns: vec![TaskFn {
                frame: frame_of_i64s(6),
                code: vec![
                    Op::ConstI64 { dst: 0, value: 2 },
                    Op::ConstI64 { dst: 8, value: 1 },
                    Op::EqI64 {
                        dst: 16,
                        a: 0,
                        b: 8,
                    },
                    Op::JumpIfZero {
                        value: 16,
                        target: 7,
                    },
                    Op::ConstI64 { dst: 24, value: 10 },
                    Op::CopyI64 { dst: 40, src: 24 },
                    Op::Jump { target: 15 },
                    Op::ConstI64 { dst: 8, value: 2 },
                    Op::EqI64 {
                        dst: 16,
                        a: 0,
                        b: 8,
                    },
                    Op::JumpIfZero {
                        value: 16,
                        target: 13,
                    },
                    Op::ConstI64 { dst: 24, value: 20 },
                    Op::CopyI64 { dst: 40, src: 24 },
                    Op::Jump { target: 15 },
                    Op::ConstI64 { dst: 24, value: 30 },
                    Op::CopyI64 { dst: 40, src: 24 },
                    Op::Ret { src: 40, size: 8 },
                ],
            }],
        };
        differential(&program, FnId(0), &[(&[], &[])]);
    }

    #[test]
    fn production_branch_target_into_trace_lands_on_next_emitted_stencil() {
        let program = Program {
            fns: vec![TaskFn {
                frame: frame_of_i64s(2),
                code: vec![
                    Op::ConstI64 { dst: 0, value: 0 },
                    Op::JumpIfZero {
                        value: 0,
                        target: 3,
                    },
                    Op::ConstI64 { dst: 8, value: 99 },
                    Op::Trace { id: 42 },
                    Op::ConstI64 { dst: 8, value: 77 },
                    Op::Ret { src: 8, size: 8 },
                ],
            }],
        };
        differential_with_mode(&program, FnId(0), &[(&[], &[])], TraceMode::Production);
    }

    fn frame_of_i64s(n: usize) -> Layout {
        Layout {
            size: n * 8,
            align: 8,
        }
    }

    #[test]
    fn straight_line_calls_match_the_interpreter() {
        let program = Program {
            fns: vec![
                TaskFn {
                    frame: frame_of_i64s(3),
                    code: vec![
                        Op::ConstI64 { dst: 0, value: 6 },
                        Op::ConstI64 { dst: 8, value: 7 },
                        Op::Call {
                            callee: FnId(1),
                            args: vec![
                                ArgCopy {
                                    src: 0,
                                    dst: 0,
                                    size: 8,
                                },
                                ArgCopy {
                                    src: 8,
                                    dst: 8,
                                    size: 8,
                                },
                            ],
                            ret: 16,
                        },
                        Op::AddI64 {
                            dst: 16,
                            a: 16,
                            b: 0,
                        },
                        Op::Ret { src: 16, size: 8 },
                    ],
                },
                TaskFn {
                    frame: frame_of_i64s(3),
                    code: vec![
                        Op::MulI64 {
                            dst: 16,
                            a: 0,
                            b: 8,
                        },
                        Op::AddI64 {
                            dst: 16,
                            a: 16,
                            b: 0,
                        },
                        Op::Ret { src: 16, size: 8 },
                    ],
                },
            ],
        };
        differential(&program, FnId(0), &[(&[], &[])]);
    }

    #[test]
    fn parking_two_frames_deep_matches_the_interpreter() {
        let program = Program {
            fns: vec![
                TaskFn {
                    frame: frame_of_i64s(2),
                    code: vec![
                        Op::ConstI64 { dst: 0, value: 100 },
                        Op::Call {
                            callee: FnId(1),
                            args: vec![],
                            ret: 8,
                        },
                        Op::AddI64 { dst: 8, a: 8, b: 0 },
                        Op::Ret { src: 8, size: 8 },
                    ],
                },
                TaskFn {
                    frame: frame_of_i64s(1),
                    code: vec![
                        Op::Await { dst: 0, input: 0 },
                        Op::AddI64 { dst: 0, a: 0, b: 0 },
                        Op::Ret { src: 0, size: 8 },
                    ],
                },
            ],
        };
        differential(&program, FnId(0), &[(&[false], &[0]), (&[true], &[21])]);
    }

    #[test]
    fn inline_composites_match_the_interpreter() {
        // The 48-byte by-value stress under native code: dynamic
        // indexing, one-ArgCopy composite pass, park with composites
        // live, value-semantics mutation isolation.
        let mut caller_code = vec![Op::ConstI64 { dst: 0, value: 7 }];
        for k in 0..6i64 {
            caller_code.push(Op::ConstI64 { dst: 64, value: k });
            caller_code.push(Op::ConstI64 {
                dst: 72,
                value: 10 * (k + 1),
            });
            caller_code.push(Op::StoreIndexedI64 {
                base: 8,
                index: 64,
                stride: 8,
                src: 72,
            });
        }
        caller_code.push(Op::Call {
            callee: FnId(1),
            args: vec![ArgCopy {
                src: 8,
                dst: 0,
                size: 48,
            }],
            ret: 56,
        });
        caller_code.push(Op::ConstI64 { dst: 64, value: 2 });
        caller_code.push(Op::LoadIndexedI64 {
            dst: 72,
            base: 8,
            index: 64,
            stride: 8,
        });
        caller_code.push(Op::AddI64 {
            dst: 56,
            a: 56,
            b: 72,
        });
        caller_code.push(Op::Ret { src: 56, size: 8 });

        let callee_code = vec![
            Op::Await { dst: 48, input: 0 },
            Op::LoadIndexedI64 {
                dst: 56,
                base: 0,
                index: 48,
                stride: 8,
            },
            Op::ConstI64 { dst: 72, value: 1 },
            Op::AddI64 {
                dst: 48,
                a: 48,
                b: 72,
            },
            Op::LoadIndexedI64 {
                dst: 64,
                base: 0,
                index: 48,
                stride: 8,
            },
            Op::AddI64 {
                dst: 72,
                a: 56,
                b: 64,
            },
            Op::ConstI64 {
                dst: 56,
                value: 999,
            },
            Op::StoreIndexedI64 {
                base: 0,
                index: 48,
                stride: 8,
                src: 56,
            },
            Op::Ret { src: 72, size: 8 },
        ];

        let program = Program {
            fns: vec![
                TaskFn {
                    frame: frame_of_i64s(10),
                    code: caller_code,
                },
                TaskFn {
                    frame: frame_of_i64s(10),
                    code: callee_code,
                },
            ],
        };
        differential(&program, FnId(0), &[(&[false], &[0]), (&[true], &[2])]);
    }

    #[test]
    fn composite_returns_match_the_interpreter() {
        let program = Program {
            fns: vec![
                TaskFn {
                    frame: Layout { size: 40, align: 8 },
                    code: vec![
                        Op::Call {
                            callee: FnId(1),
                            args: vec![],
                            ret: 0,
                        },
                        Op::ConstI64 { dst: 24, value: 1 },
                        Op::LoadIndexedI64 {
                            dst: 32,
                            base: 0,
                            index: 24,
                            stride: 8,
                        },
                        Op::Ret { src: 32, size: 8 },
                    ],
                },
                TaskFn {
                    frame: Layout { size: 40, align: 8 },
                    code: vec![
                        Op::ConstI64 { dst: 24, value: 0 },
                        Op::ConstI64 { dst: 32, value: 5 },
                        Op::StoreIndexedI64 {
                            base: 0,
                            index: 24,
                            stride: 8,
                            src: 32,
                        },
                        Op::ConstI64 { dst: 24, value: 1 },
                        Op::ConstI64 { dst: 32, value: 6 },
                        Op::StoreIndexedI64 {
                            base: 0,
                            index: 24,
                            stride: 8,
                            src: 32,
                        },
                        Op::ConstI64 { dst: 24, value: 2 },
                        Op::ConstI64 { dst: 32, value: 7 },
                        Op::StoreIndexedI64 {
                            base: 0,
                            index: 24,
                            stride: 8,
                            src: 32,
                        },
                        Op::Ret { src: 0, size: 24 },
                    ],
                },
            ],
        };
        differential(&program, FnId(0), &[(&[], &[])]);
    }
}
