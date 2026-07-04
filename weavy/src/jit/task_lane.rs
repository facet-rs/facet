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
use crate::task::{ArgCopy, FnId, Op, Program, TaskEvent, TaskStep};

/// Threaded state — MUST match `Ctx` in stencils/task_ops.rs.
#[repr(C)]
struct Ctx {
    prog: *const u64,
    frame: *mut u8,
    ready: *const i64,
    awaited: *const i64,
    resume: *mut u64,
    await_index: *mut u64,
    exit: *mut i64,
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
    /// Compile every function. Returns None when the lane is
    /// unavailable on this target.
    pub fn compile(program: &Program) -> Option<JitProgram> {
        if !available() {
            return None;
        }
        let fns = program.fns.iter().map(compile_fn).collect();
        Some(JitProgram { fns })
    }
}

fn compile_fn(f: &crate::task::Fn) -> CompiledFn {
    let mut layout = StencilLayout::new();
    let root = layout.start_chain();

    // Pass 1: code. Record each op's chain start and continuation
    // patch sites; collect call descriptors keyed by continuation.
    let mut starts = Vec::with_capacity(f.code.len());
    let mut sites: Vec<(usize, &'static [usize])> = Vec::new();
    for op in &f.code {
        let (bytes, cont): (&[u8], &'static [usize]) = match op {
            Op::ConstI64 { .. } => (task_stencils::CONST, task_stencils::CONST_CONT),
            Op::AddI64 { .. } => (task_stencils::ADD, task_stencils::ADD_CONT),
            Op::MulI64 { .. } => (task_stencils::MUL, task_stencils::MUL_CONT),
            Op::LoadIndexedI64 { .. } => (task_stencils::LOAD_IX, task_stencils::LOAD_IX_CONT),
            Op::StoreIndexedI64 { .. } => (task_stencils::STORE_IX, task_stencils::STORE_IX_CONT),
            Op::Await { .. } => (task_stencils::AWAIT, task_stencils::AWAIT_CONT),
            Op::Call { .. } => (task_stencils::CALL, task_stencils::CALL_CONT),
            Op::Ret { .. } => (task_stencils::RET, task_stencils::RET_CONT),
        };
        let start = layout.emit_stencil(bytes);
        starts.push(start);
        sites.push((start, cont));
    }
    let done = layout.emit_stencil(task_stencils::DONE);
    for (i, &(start, cont)) in sites.iter().enumerate() {
        let target = starts.get(i + 1).copied().unwrap_or(done);
        for &rel in cont {
            layout.patch_continuation(start + rel, target);
        }
    }

    // Pass 2: the immediate stream, in op order (consumption order).
    let mut calls = HashMap::new();
    for (i, op) in f.code.iter().enumerate() {
        match op {
            Op::ConstI64 { dst, value } => {
                layout.push_prog_word(root.prog_index, u64::from(*dst));
                layout.push_prog_word(root.prog_index, *value as u64);
            }
            Op::AddI64 { dst, a, b } | Op::MulI64 { dst, a, b } => {
                layout.push_prog_word(root.prog_index, u64::from(*dst));
                layout.push_prog_word(root.prog_index, u64::from(*a));
                layout.push_prog_word(root.prog_index, u64::from(*b));
            }
            Op::LoadIndexedI64 { dst, base, index, stride } => {
                for v in [dst, base, index, stride] {
                    layout.push_prog_word(root.prog_index, u64::from(*v));
                }
            }
            Op::StoreIndexedI64 { base, index, stride, src } => {
                for v in [base, index, stride, src] {
                    layout.push_prog_word(root.prog_index, u64::from(*v));
                }
            }
            Op::Await { dst, input } => {
                // [resume_off = own start, index, dst] — idempotent
                // suspend point, the proven protocol.
                layout.push_prog_word(root.prog_index, starts[i] as u64);
                layout.push_prog_word(root.prog_index, u64::from(*input));
                layout.push_prog_word(root.prog_index, u64::from(*dst));
            }
            Op::Call { callee, args, ret } => {
                // [continuation = NEXT op's start]; descriptor lives in
                // the side table under that same key.
                let continuation = starts.get(i + 1).copied().unwrap_or(done) as u64;
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

    fn alloc_frame(&mut self, f: &CompiledFn) -> usize {
        let align = f.frame_align.max(1);
        let base = self.arena.len().div_ceil(align) * align;
        self.arena.resize(base + f.frame_size, 0);
        base
    }

    pub fn run(&mut self, program: &JitProgram, ready: &[bool], awaited: &[i64]) -> TaskStep {
        self.ready_scratch.clear();
        self.ready_scratch.extend(ready.iter().map(|&r| i64::from(r)));
        if let Some(input) = self.parked_on
            && ready.get(input as usize).copied().unwrap_or(false)
        {
            self.parked_on = None;
            self.trace.push(TaskEvent::Resumed);
        }
        loop {
            let frame = self.frames.last().expect("running task has a frame").clone();
            let compiled = &program.fns[frame.fn_id.0 as usize];

            let entry_prog = compiled.native.entry_prog();
            let mut resume_scratch = 0u64;
            let mut index_scratch = 0u64;
            let mut exit_scratch = 0i64;
            let arena_base = self.arena.as_mut_ptr();
            let mut ctx = Ctx {
                prog: unsafe { entry_prog.add(frame.prog_pos) },
                frame: unsafe { arena_base.add(frame.base) },
                ready: self.ready_scratch.as_ptr(),
                awaited: awaited.as_ptr(),
                resume: &mut resume_scratch,
                await_index: &mut index_scratch,
                exit: &mut exit_scratch,
            };
            // SAFETY: `frame.resume` is a chain offset of this compiled
            // function; the copied code uses the extern "C" fn(*mut Ctx)
            // ABI its stencils were built with. No arena allocation
            // happens while the chain runs (driver-only allocation).
            let f = unsafe { compiled.native.chain_fn::<Ctx>(frame.resume) };
            unsafe { f(&mut ctx) };
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
                code => panic!("task chain exited with code {code} (fell through without Ret?)"),
            }
        }
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mem::Layout;
    use crate::task::{Fn as TaskFn, Task};

    /// Drive interp and JIT through the same schedule; assert identical
    /// steps, results, and traces.
    fn differential(
        program: &Program,
        entry: FnId,
        schedule: &[(&[bool], &[i64])],
    ) {
        let mut interp = Task::spawn(program, entry);
        let mut interp_steps = Vec::new();
        for (ready, awaited) in schedule {
            let step = interp.run(program, ready, awaited);
            interp_steps.push(step);
            if step == TaskStep::Done {
                break;
            }
        }

        let Some(jit) = JitProgram::compile(program) else {
            return; // no JIT on this target — interp already asserted by task::tests
        };
        let mut task = JitTask::spawn(&jit, entry);
        let mut jit_steps = Vec::new();
        for (ready, awaited) in schedule {
            let step = task.run(&jit, ready, awaited);
            jit_steps.push(step);
            if step == TaskStep::Done {
                break;
            }
        }

        assert_eq!(jit_steps, interp_steps, "step sequences diverge");
        assert_eq!(task.result, interp.result, "results diverge");
        assert_eq!(task.trace, interp.trace, "frame traces diverge");
    }

    fn frame_of_i64s(n: usize) -> Layout {
        Layout { size: n * 8, align: 8 }
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
                                ArgCopy { src: 0, dst: 0, size: 8 },
                                ArgCopy { src: 8, dst: 8, size: 8 },
                            ],
                            ret: 16,
                        },
                        Op::AddI64 { dst: 16, a: 16, b: 0 },
                        Op::Ret { src: 16, size: 8 },
                    ],
                },
                TaskFn {
                    frame: frame_of_i64s(3),
                    code: vec![
                        Op::MulI64 { dst: 16, a: 0, b: 8 },
                        Op::AddI64 { dst: 16, a: 16, b: 0 },
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
                        Op::Call { callee: FnId(1), args: vec![], ret: 8 },
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
        differential(
            &program,
            FnId(0),
            &[(&[false], &[0]), (&[true], &[21])],
        );
    }

    #[test]
    fn inline_composites_match_the_interpreter() {
        // The 48-byte by-value stress under native code: dynamic
        // indexing, one-ArgCopy composite pass, park with composites
        // live, value-semantics mutation isolation.
        let mut caller_code = vec![Op::ConstI64 { dst: 0, value: 7 }];
        for k in 0..6i64 {
            caller_code.push(Op::ConstI64 { dst: 64, value: k });
            caller_code.push(Op::ConstI64 { dst: 72, value: 10 * (k + 1) });
            caller_code.push(Op::StoreIndexedI64 { base: 8, index: 64, stride: 8, src: 72 });
        }
        caller_code.push(Op::Call {
            callee: FnId(1),
            args: vec![ArgCopy { src: 8, dst: 0, size: 48 }],
            ret: 56,
        });
        caller_code.push(Op::ConstI64 { dst: 64, value: 2 });
        caller_code.push(Op::LoadIndexedI64 { dst: 72, base: 8, index: 64, stride: 8 });
        caller_code.push(Op::AddI64 { dst: 56, a: 56, b: 72 });
        caller_code.push(Op::Ret { src: 56, size: 8 });

        let callee_code = vec![
            Op::Await { dst: 48, input: 0 },
            Op::LoadIndexedI64 { dst: 56, base: 0, index: 48, stride: 8 },
            Op::ConstI64 { dst: 72, value: 1 },
            Op::AddI64 { dst: 48, a: 48, b: 72 },
            Op::LoadIndexedI64 { dst: 64, base: 0, index: 48, stride: 8 },
            Op::AddI64 { dst: 72, a: 56, b: 64 },
            Op::ConstI64 { dst: 56, value: 999 },
            Op::StoreIndexedI64 { base: 0, index: 48, stride: 8, src: 56 },
            Op::Ret { src: 72, size: 8 },
        ];

        let program = Program {
            fns: vec![
                TaskFn { frame: frame_of_i64s(10), code: caller_code },
                TaskFn { frame: frame_of_i64s(10), code: callee_code },
            ],
        };
        differential(
            &program,
            FnId(0),
            &[(&[false], &[0]), (&[true], &[2])],
        );
    }

    #[test]
    fn composite_returns_match_the_interpreter() {
        let program = Program {
            fns: vec![
                TaskFn {
                    frame: Layout { size: 40, align: 8 },
                    code: vec![
                        Op::Call { callee: FnId(1), args: vec![], ret: 0 },
                        Op::ConstI64 { dst: 24, value: 1 },
                        Op::LoadIndexedI64 { dst: 32, base: 0, index: 24, stride: 8 },
                        Op::Ret { src: 32, size: 8 },
                    ],
                },
                TaskFn {
                    frame: Layout { size: 40, align: 8 },
                    code: vec![
                        Op::ConstI64 { dst: 24, value: 0 },
                        Op::ConstI64 { dst: 32, value: 5 },
                        Op::StoreIndexedI64 { base: 0, index: 24, stride: 8, src: 32 },
                        Op::ConstI64 { dst: 24, value: 1 },
                        Op::ConstI64 { dst: 32, value: 6 },
                        Op::StoreIndexedI64 { base: 0, index: 24, stride: 8, src: 32 },
                        Op::ConstI64 { dst: 24, value: 2 },
                        Op::ConstI64 { dst: 32, value: 7 },
                        Op::StoreIndexedI64 { base: 0, index: 24, stride: 8, src: 32 },
                        Op::Ret { src: 0, size: 24 },
                    ],
                },
            ],
        };
        differential(&program, FnId(0), &[(&[], &[])]);
    }
}
