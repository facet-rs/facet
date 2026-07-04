//! Tasks, frames, and the typed calling convention — tooth 2 of the
//! substrate, per the ruled ABI (vixen repo, docs/design/
//! tooth-2-frames-abi.md).
//!
//! - **A frame is a declared record.** Its layout (args, locals, spill
//!   slots) is computed by the same machinery as any record
//!   (mem::declared); code addresses it by statically known byte
//!   offsets. The debugger reads frames the way it reads values.
//! - **Frames live in a per-task arena**, never on the C stack.
//!   Parking a task costs nothing: the live frame chain already sits
//!   in the arena — stop running and the state is simply still there.
//! - **Arguments travel frame-direct**: the caller writes each
//!   argument into the callee's frame at its known offset — typed
//!   stores, no marshalling. Composite returns go through a
//!   caller-designated return slot (sret shape).
//! - **The await-spill rule**: at an await point every live value is
//!   in a frame. In THIS lane it holds by construction — the
//!   instruction set is three-address over frame offsets, values are
//!   always frame-resident. Stencil lanes may cache registers between
//!   awaits; the rule constrains them at await sites.
//! - **Sync vs async sites are distinct in the ABI** (Amos's
//!   refinement): only [`Op::Await`] sites carry await-point
//!   machinery; synchronous host calls will be a separate op with no
//!   park path, no numbering, no spill obligations.
//! - **Typed instructions over untagged operands** (constitution A6):
//!   the arena is raw bytes; ops imply types; nothing is
//!   self-describing at runtime.
//!
//! Trace events are first-class (frame-granular, per the ruling); in
//! this slice they are recorded directly — the strippable
//! IR-instrumentation form arrives with the trace-vocabulary slice.

use crate::mem::Layout;

/// Identifies a function in a [`Program`]'s function table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FnId(pub u32);

/// A function: its frame's layout (a declared record of args, locals,
/// and spill slots — offsets are the callers' and body's shared
/// knowledge) and its code.
#[derive(Clone, Debug)]
pub struct Fn {
    pub frame: Layout,
    pub code: Vec<Op>,
}

/// A program: functions addressed by [`FnId`].
#[derive(Clone, Debug, Default)]
pub struct Program {
    pub fns: Vec<Fn>,
}

/// One argument copy of a frame-direct call: `size` bytes from the
/// caller's frame at `src` into the callee's frame at `dst`. Emitted
/// by a lowering that statically knows both layouts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArgCopy {
    pub src: u32,
    pub dst: u32,
    pub size: u32,
}

/// Typed, three-address instructions over frame offsets. The
/// vocabulary grows per kind (AddF64, loads/stores of declared
/// fields, sync host calls) — per the ruled stencil order, frame/call/
/// return machinery lands before arithmetic variety.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    /// `frame[dst] = value` (i64).
    ConstI64 { dst: u32, value: i64 },
    /// `frame[dst] = frame[a] + frame[b]` (i64, wrapping).
    AddI64 { dst: u32, a: u32, b: u32 },
    /// `frame[dst] = frame[a] * frame[b]` (i64, wrapping).
    MulI64 { dst: u32, a: u32, b: u32 },
    /// Frame-direct call: allocate the callee's frame in the task
    /// arena, copy `args`, enter. The callee's `Ret` writes `size`
    /// bytes into THIS frame at `ret`.
    Call {
        callee: FnId,
        args: Vec<ArgCopy>,
        ret: u32,
    },
    /// Return `size` bytes at `src` to the caller's designated return
    /// slot (or to the task result if this is the root frame), then
    /// pop the frame.
    Ret { src: u32, size: u32 },
    /// ASYNC await point (numbered in task order of first arrival):
    /// if `input` is ready, write its value (i64 in this slice) to
    /// `frame[dst]` and continue; otherwise PARK the task. Sync host
    /// calls are deliberately NOT this op.
    Await { dst: u32, input: u32 },
}

/// Frame-granular trace events (the ruled vocabulary, recorded
/// directly in this slice).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskEvent {
    FrameEntered(FnId),
    FrameExited(FnId),
    Parked { input: u32 },
    Resumed,
}

/// The result of driving a task.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskStep {
    /// The root frame returned; the result is in [`Task::result`].
    Done,
    /// Parked on an unready input — started-and-blocked, the only
    /// kind of suspension that exists.
    Parked { input: u32 },
}

#[derive(Clone, Debug)]
struct FrameRecord {
    fn_id: FnId,
    /// Arena offset of this frame's first byte.
    base: usize,
    pc: usize,
    /// Absolute arena offset in the CALLER's frame where our `Ret`
    /// writes; `None` for the root frame (writes to the task result).
    ret_to: Option<usize>,
}

/// A task: a frame arena, the live frame chain, and the trace. This
/// struct IS the suspended state — parking is returning.
#[derive(Clone, Debug)]
pub struct Task {
    arena: Vec<u8>,
    frames: Vec<FrameRecord>,
    /// Root return bytes once [`TaskStep::Done`].
    pub result: Vec<u8>,
    pub trace: Vec<TaskEvent>,
    parked_on: Option<u32>,
}

impl Task {
    /// Spawn with the entry function's frame allocated. Callers of the
    /// task write entry arguments through [`Task::write_i64`] before
    /// the first [`Task::run`] — the frame-direct convention applies
    /// at the boundary too.
    #[must_use]
    pub fn spawn(program: &Program, entry: FnId) -> Self {
        let mut task = Task {
            arena: Vec::new(),
            frames: Vec::new(),
            result: Vec::new(),
            trace: Vec::new(),
            parked_on: None,
        };
        let base = task.alloc_frame(program.fns[entry.0 as usize].frame);
        task.frames.push(FrameRecord {
            fn_id: entry,
            base,
            pc: 0,
            ret_to: None,
        });
        task.trace.push(TaskEvent::FrameEntered(entry));
        task
    }

    /// Live frame count (the chain that survives parking).
    #[must_use]
    pub fn depth(&self) -> usize {
        self.frames.len()
    }

    /// Write an i64 into the CURRENT frame at `offset` — used for
    /// entry arguments and by tests to poke frames.
    pub fn write_i64(&mut self, offset: u32, value: i64) {
        let base = self.frames.last().expect("live frame").base;
        write_i64_at(&mut self.arena, base + offset as usize, value);
    }

    /// Read an i64 from the task result (root return bytes).
    #[must_use]
    pub fn result_i64(&self) -> i64 {
        i64::from_le_bytes(self.result[..8].try_into().expect("8-byte result"))
    }

    fn alloc_frame(&mut self, layout: Layout) -> usize {
        let align = layout.align.max(1);
        let base = self.arena.len().div_ceil(align) * align;
        self.arena.resize(base + layout.size, 0);
        base
    }

    /// Drive until the root returns or the task parks. `ready` and
    /// `awaited` are indexed by await input, exactly as in the proven
    /// suspend protocol.
    pub fn run(&mut self, program: &Program, ready: &[bool], awaited: &[i64]) -> TaskStep {
        loop {
            let frame = self.frames.last().expect("running task has a frame");
            let base = frame.base;
            let code = &program.fns[frame.fn_id.0 as usize].code;
            if frame.pc >= code.len() {
                panic!("function {:?} fell off its code without Ret", frame.fn_id);
            }
            match code[frame.pc].clone() {
                Op::ConstI64 { dst, value } => {
                    write_i64_at(&mut self.arena, base + dst as usize, value);
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::AddI64 { dst, a, b } => {
                    let va = read_i64_at(&self.arena, base + a as usize);
                    let vb = read_i64_at(&self.arena, base + b as usize);
                    write_i64_at(&mut self.arena, base + dst as usize, va.wrapping_add(vb));
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::MulI64 { dst, a, b } => {
                    let va = read_i64_at(&self.arena, base + a as usize);
                    let vb = read_i64_at(&self.arena, base + b as usize);
                    write_i64_at(&mut self.arena, base + dst as usize, va.wrapping_mul(vb));
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::Call { callee, args, ret } => {
                    // Advance the caller past the call BEFORE entering:
                    // resumption re-enters the callee, and the caller
                    // continues after the callee's Ret.
                    self.frames.last_mut().expect("frame").pc += 1;
                    let callee_frame = self.alloc_frame(program.fns[callee.0 as usize].frame);
                    for copy in &args {
                        // Frame-direct: caller bytes land at the
                        // callee's statically known offsets.
                        let src = base + copy.src as usize;
                        let dst = callee_frame + copy.dst as usize;
                        self.arena
                            .copy_within(src..src + copy.size as usize, dst);
                    }
                    self.frames.push(FrameRecord {
                        fn_id: callee,
                        base: callee_frame,
                        pc: 0,
                        ret_to: Some(base + ret as usize),
                    });
                    self.trace.push(TaskEvent::FrameEntered(callee));
                }
                Op::Ret { src, size } => {
                    let popped = self.frames.pop().expect("frame to return from");
                    self.trace.push(TaskEvent::FrameExited(popped.fn_id));
                    let start = popped.base + src as usize;
                    match popped.ret_to {
                        Some(ret_to) => {
                            self.arena.copy_within(start..start + size as usize, ret_to);
                        }
                        None => {
                            self.result = self.arena[start..start + size as usize].to_vec();
                            return TaskStep::Done;
                        }
                    }
                }
                Op::Await { dst, input } => {
                    let idx = input as usize;
                    if ready.get(idx).copied().unwrap_or(false) {
                        if self.parked_on == Some(input) {
                            self.parked_on = None;
                            self.trace.push(TaskEvent::Resumed);
                        }
                        write_i64_at(&mut self.arena, base + dst as usize, awaited[idx]);
                        self.frames.last_mut().expect("frame").pc += 1;
                    } else {
                        // Started-and-blocked: the arena and frame
                        // chain ARE the suspended state; leave pc AT
                        // the await so resume re-checks it.
                        if self.parked_on != Some(input) {
                            self.parked_on = Some(input);
                            self.trace.push(TaskEvent::Parked { input });
                        }
                        return TaskStep::Parked { input };
                    }
                }
            }
        }
    }
}

fn read_i64_at(arena: &[u8], at: usize) -> i64 {
    i64::from_le_bytes(arena[at..at + 8].try_into().expect("aligned i64 slot"))
}

fn write_i64_at(arena: &mut [u8], at: usize, value: i64) {
    arena[at..at + 8].copy_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mem::declared::{declared_struct, i64_};
    use crate::mem::Access;

    fn frame_of_i64s(n: usize) -> Layout {
        Layout {
            size: n * 8,
            align: 8,
        }
    }

    /// callee(x, y) at offsets 0,8 -> returns (x*y)+x from slot 16.
    fn mul_plus_x() -> Fn {
        Fn {
            frame: frame_of_i64s(3),
            code: vec![
                Op::MulI64 { dst: 16, a: 0, b: 8 },
                Op::AddI64 { dst: 16, a: 16, b: 0 },
                Op::Ret { src: 16, size: 8 },
            ],
        }
    }

    #[test]
    fn frame_direct_calls_compute_and_trace_frames() {
        // outer: locals a@0=6, b@8=7; calls callee(a,b) -> ret@16;
        // returns ret+a.
        let program = Program {
            fns: vec![
                Fn {
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
                mul_plus_x(),
            ],
        };
        let mut task = Task::spawn(&program, FnId(0));
        assert_eq!(task.run(&program, &[], &[]), TaskStep::Done);
        // (6*7)+6 = 48, +6 again in the caller = 54.
        assert_eq!(task.result_i64(), 54);
        assert_eq!(
            task.trace,
            vec![
                TaskEvent::FrameEntered(FnId(0)),
                TaskEvent::FrameEntered(FnId(1)),
                TaskEvent::FrameExited(FnId(1)),
                TaskEvent::FrameExited(FnId(0)),
            ]
        );
    }

    #[test]
    fn parking_preserves_the_live_frame_chain() {
        // outer: local salt@0=100; calls callee -> ret@8; returns
        // ret+salt. callee: awaits input#0 into 0, doubles it, returns.
        // The park happens two frames deep; the caller's local must
        // survive in the arena across the suspension.
        let program = Program {
            fns: vec![
                Fn {
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
                Fn {
                    frame: frame_of_i64s(1),
                    code: vec![
                        Op::Await { dst: 0, input: 0 },
                        Op::AddI64 { dst: 0, a: 0, b: 0 },
                        Op::Ret { src: 0, size: 8 },
                    ],
                },
            ],
        };
        let mut task = Task::spawn(&program, FnId(0));

        assert_eq!(task.run(&program, &[false], &[0]), TaskStep::Parked { input: 0 });
        assert_eq!(task.depth(), 2, "both frames live while parked");
        assert!(task.trace.contains(&TaskEvent::Parked { input: 0 }));

        // The task struct IS the suspended state; nothing else exists.
        assert_eq!(task.run(&program, &[true], &[21]), TaskStep::Done);
        assert_eq!(task.result_i64(), 21 * 2 + 100);
        assert!(task.trace.contains(&TaskEvent::Resumed));
        let exits: Vec<_> = task
            .trace
            .iter()
            .filter(|e| matches!(e, TaskEvent::FrameExited(_)))
            .collect();
        assert_eq!(exits.len(), 2);
    }

    #[test]
    fn ready_awaits_never_park() {
        let program = Program {
            fns: vec![Fn {
                frame: frame_of_i64s(1),
                code: vec![
                    Op::Await { dst: 0, input: 0 },
                    Op::Ret { src: 0, size: 8 },
                ],
            }],
        };
        let mut task = Task::spawn(&program, FnId(0));
        assert_eq!(task.run(&program, &[true], &[42]), TaskStep::Done);
        assert_eq!(task.result_i64(), 42);
        assert!(!task.trace.iter().any(|e| matches!(e, TaskEvent::Parked { .. })));
    }

    #[test]
    fn frame_layouts_come_from_declared_records() {
        // A callee frame declared as a record of (x: i64, y: i64,
        // out: i64): the lowering-side story — ArgCopy dst offsets ARE
        // the declared record's field offsets.
        let frame_desc = declared_struct((), vec![i64_(()), i64_(()), i64_(())]);
        let Access::Record(record) = &frame_desc.access else {
            panic!("record expected");
        };
        let x = u32::try_from(record.fields[0].offset).unwrap();
        let y = u32::try_from(record.fields[1].offset).unwrap();
        let out = u32::try_from(record.fields[2].offset).unwrap();

        let program = Program {
            fns: vec![
                Fn {
                    frame: frame_of_i64s(3),
                    code: vec![
                        Op::ConstI64 { dst: 0, value: 6 },
                        Op::ConstI64 { dst: 8, value: 9 },
                        Op::Call {
                            callee: FnId(1),
                            args: vec![
                                ArgCopy { src: 0, dst: x, size: 8 },
                                ArgCopy { src: 8, dst: y, size: 8 },
                            ],
                            ret: 16,
                        },
                        Op::Ret { src: 16, size: 8 },
                    ],
                },
                Fn {
                    frame: frame_desc.layout,
                    code: vec![
                        Op::MulI64 { dst: out, a: x, b: y },
                        Op::Ret { src: out, size: 8 },
                    ],
                },
            ],
        };
        let mut task = Task::spawn(&program, FnId(0));
        assert_eq!(task.run(&program, &[], &[]), TaskStep::Done);
        assert_eq!(task.result_i64(), 54);
    }

    #[test]
    fn three_deep_calls_return_through_designated_slots() {
        // f0 -> f1 -> f2; each adds its own constant.
        let leaf = Fn {
            frame: frame_of_i64s(2),
            code: vec![
                Op::ConstI64 { dst: 8, value: 1 },
                Op::AddI64 { dst: 0, a: 0, b: 8 },
                Op::Ret { src: 0, size: 8 },
            ],
        };
        let mid = Fn {
            frame: frame_of_i64s(2),
            code: vec![
                Op::Call {
                    callee: FnId(2),
                    args: vec![ArgCopy { src: 0, dst: 0, size: 8 }],
                    ret: 8,
                },
                Op::AddI64 { dst: 8, a: 8, b: 0 },
                Op::Ret { src: 8, size: 8 },
            ],
        };
        let root = Fn {
            frame: frame_of_i64s(2),
            code: vec![
                Op::ConstI64 { dst: 0, value: 10 },
                Op::Call {
                    callee: FnId(1),
                    args: vec![ArgCopy { src: 0, dst: 0, size: 8 }],
                    ret: 8,
                },
                Op::Ret { src: 8, size: 8 },
            ],
        };
        let program = Program {
            fns: vec![root, mid, leaf],
        };
        let mut task = Task::spawn(&program, FnId(0));
        assert_eq!(task.run(&program, &[], &[]), TaskStep::Done);
        // leaf: 10+1=11; mid: 11+10=21.
        assert_eq!(task.result_i64(), 21);
        assert_eq!(task.depth(), 0);
    }
}
