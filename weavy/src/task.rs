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

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

use crate::mem::Layout;

/// One immutable value payload made visible to task code for native reads.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ValueMemory {
    pub ptr: *const u8,
    pub len: usize,
}

impl ValueMemory {
    #[must_use]
    pub fn from_slice(bytes: &[u8]) -> Self {
        Self {
            ptr: bytes.as_ptr(),
            len: bytes.len(),
        }
    }

    #[must_use]
    pub fn empty() -> Self {
        Self {
            ptr: core::ptr::null(),
            len: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ValueMemories<'a> {
    pub store: &'a [ValueMemory],
    pub molten: &'a [ValueMemory],
}

impl ValueMemories<'_> {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            store: &[],
            molten: &[],
        }
    }
}

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
    /// `frame[dst] = frame[a] - frame[b]` (i64, wrapping).
    SubI64 { dst: u32, a: u32, b: u32 },
    /// `frame[dst] = frame[a] * frame[b]` (i64, wrapping).
    MulI64 { dst: u32, a: u32, b: u32 },
    /// Total wrapping division: zero maps to zero and `MIN / -1` maps to `MIN`.
    DivI64 { dst: u32, a: u32, b: u32 },
    /// `frame[dst] = frame[src]` (one 64-bit word).
    CopyI64 { dst: u32, src: u32 },
    /// `frame[dst] = (frame[a] == frame[b]) as i64`.
    EqI64 { dst: u32, a: u32, b: u32 },
    /// `frame[dst] = (frame[a] != frame[b]) as i64`.
    NeI64 { dst: u32, a: u32, b: u32 },
    /// `frame[dst] = (frame[a] < frame[b]) as i64`.
    LtI64 { dst: u32, a: u32, b: u32 },
    /// `frame[dst] = (frame[a] <= frame[b]) as i64`.
    LeI64 { dst: u32, a: u32, b: u32 },
    /// `frame[dst] = (frame[a] > frame[b]) as i64`.
    GtI64 { dst: u32, a: u32, b: u32 },
    /// `frame[dst] = (frame[a] >= frame[b]) as i64`.
    GeI64 { dst: u32, a: u32, b: u32 },
    /// Continue at absolute instruction index `target` in the current function.
    Jump { target: u32 },
    /// Continue at `target` when `frame[value] == 0`, otherwise fall through.
    JumpIfZero { value: u32, target: u32 },
    /// Frame-direct call: allocate the callee's frame in the task
    /// arena, copy `args`, enter. The callee's `Ret` writes `size`
    /// bytes into THIS frame at `ret`.
    Call {
        callee: FnId,
        args: Vec<ArgCopy>,
        ret: u32,
    },
    /// Frame-direct call through a closure's local function-id word.
    CallIndirect {
        callee: u32,
        args: Vec<ArgCopy>,
        ret: u32,
    },
    /// Return `size` bytes at `src` to the caller's designated return
    /// slot (or to the task result if this is the root frame), then
    /// pop the frame.
    Ret { src: u32, size: u32 },
    /// ASYNC await point (numbered in task order of first arrival):
    /// if `input` is ready, consume that ready token, write its value
    /// (i64 in this slice) to `frame[dst]`, and continue; otherwise PARK the task. Sync host
    /// calls are deliberately NOT this op.
    Await { dst: u32, input: u32 },
    /// `frame[dst] = frame[base + frame[index]*stride]` — dynamic
    /// indexing into an INLINE composite (an array living in the
    /// frame, unboxed). Bounds are the checker's obligation: the
    /// count is static in the array's declared layout; a lowering
    /// that emits an out-of-range index has a compiler bug, and a
    /// validation pass may reject programs statically — never a
    /// runtime tag or check here (constitution A6).
    LoadIndexedI64 {
        dst: u32,
        base: u32,
        index: u32,
        stride: u32,
    },
    /// `frame[base + frame[index]*stride] = frame[src]` — the store
    /// twin of [`Op::LoadIndexedI64`], same obligations.
    StoreIndexedI64 {
        base: u32,
        index: u32,
        stride: u32,
        src: u32,
    },
    /// Checked read from a store-backed `Array<T>` word payload.
    ///
    /// `frame[array]` is a store handle. The value-memory table entry
    /// at that handle must be an array-words payload with matching
    /// `elem_schema_ref`. In-bounds reads write the element to `dst`
    /// and `1` to `present`; misses write zeroes to both.
    LoadArrayWord {
        dst: u32,
        present: u32,
        array: u32,
        index: u32,
        elem_schema_ref: i64,
    },
    /// Lexicographically compare two resident value-memory byte runs.
    ///
    /// `frame[a]` and `frame[b]` are value handles. The result is the closed
    /// three-way ordinal `0 = less`, `1 = equal`, `2 = greater`. Equal handles
    /// short-circuit without reading either body. For distinct handles, task
    /// admission must have made both bodies resident in the value-memory table;
    /// violating that contract is a driver bug.
    CompareValueBytes { dst: u32, a: u32, b: u32 },
    /// `frame[dst] = f64::from_bits(bits)` — the immediate carries the
    /// BIT PATTERN (keeps `Op: Eq`; the machine is type-blind about a
    /// 64-bit store anyway — the op exists so lowerings and readers
    /// see intent). Total-order/NaN canonicalization is the LANGUAGE's
    /// value-level concern (vix's TotalF64), not the machine's:
    /// arithmetic here is plain IEEE, identical across lanes.
    ConstF64 { dst: u32, bits: u64 },
    /// `frame[dst] = frame[a] + frame[b]` (f64, IEEE).
    AddF64 { dst: u32, a: u32, b: u32 },
    /// `frame[dst] = frame[a] * frame[b]` (f64, IEEE).
    MulF64 { dst: u32, a: u32, b: u32 },
    /// INSTRUMENTATION (the unified-trace ruling): lowerings emit
    /// trace marks freely; the MODE decides their cost. Innards mode
    /// records [`TaskEvent::Mark`]; Production mode strips them — in
    /// the interpreter a skip, in the JIT the op is simply NOT
    /// COMPILED (zero instructions in the chain). Static ids map back
    /// to source constructs in the lowering's tables.
    Trace { id: u32 },
    /// SYNC host call (Amos's refinement, ruled): a host operation
    /// that ALWAYS completes — no await-point numbering, no park
    /// machinery, no spill obligations beyond frame residency (which
    /// three-address gives anyway). The host function receives the
    /// current frame's bytes and reads/writes at offsets its contract
    /// (known to the lowering) declares — the frame-direct convention
    /// extended to the host boundary. `host` indexes the table passed
    /// to [`Task::run_hosted`].
    HostCall { host: u32 },
    /// Sync host call that yields to the outer driver after completion.
    ///
    /// Use this when host effects change native value-memory provenance and
    /// the next machine op may read through that provenance.
    HostCallYield { host: u32 },
}

/// A synchronous host operation over the current frame's bytes.
pub type HostFn<'h> = &'h mut dyn FnMut(&mut [u8]);

/// An owned sync host operation, as [`TaskExec`] carries them.
pub type BoxedHostFn<'h> = Box<dyn FnMut(&mut [u8]) + 'h>;

/// Frame-granular trace events (the ruled vocabulary, recorded
/// directly in this slice).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskEvent {
    FrameEntered(FnId),
    FrameExited(FnId),
    Parked {
        input: u32,
    },
    Resumed,
    /// An [`Op::Trace`] instrumentation mark (Innards mode only).
    Mark(u32),
}

/// How much instrumentation a program instance carries. Tests trace
/// innards; production keeps only the structural events (frames,
/// parks) needed for observability.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TraceMode {
    /// Record every instrumentation mark.
    #[default]
    Innards,
    /// Strip instrumentation marks entirely.
    Production,
}

/// The result of driving a task.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskStep {
    /// The root frame returned; the result is in [`Task::result`].
    Done,
    /// A sync host call completed and the task can be re-entered immediately.
    Yielded,
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
    mode: TraceMode,
}

impl Task {
    /// Spawn with the entry function's frame allocated. Callers of the
    /// task write entry arguments through [`Task::write_i64`] before
    /// the first [`Task::run`] — the frame-direct convention applies
    /// at the boundary too.
    #[must_use]
    pub fn spawn(program: &Program, entry: FnId) -> Self {
        Self::spawn_with_mode(program, entry, TraceMode::Innards)
    }

    #[must_use]
    pub fn spawn_with_mode(program: &Program, entry: FnId, mode: TraceMode) -> Self {
        let mut task = Task {
            arena: Vec::new(),
            frames: Vec::new(),
            result: Vec::new(),
            trace: Vec::new(),
            parked_on: None,
            mode,
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
    /// suspend protocol. A ready slot is consumed when its await reads
    /// it. Programs containing [`Op::HostCall`] must use [`Task::run_hosted`].
    pub fn run(&mut self, program: &Program, ready: &mut [bool], awaited: &[i64]) -> TaskStep {
        self.run_hosted(program, ready, awaited, &mut [])
    }

    /// [`Task::run`] with a host table for sync host calls.
    pub fn run_hosted(
        &mut self,
        program: &Program,
        ready: &mut [bool],
        awaited: &[i64],
        hosts: &mut [HostFn<'_>],
    ) -> TaskStep {
        self.run_hosted_with_value_memories(program, ready, awaited, hosts, ValueMemories::empty())
    }

    pub fn run_hosted_with_value_memories(
        &mut self,
        program: &Program,
        ready: &mut [bool],
        awaited: &[i64],
        hosts: &mut [HostFn<'_>],
        value_memories: ValueMemories<'_>,
    ) -> TaskStep {
        loop {
            let frame = self.frames.last().expect("running task has a frame");
            let base = frame.base;
            let fn_id = frame.fn_id;
            let code = &program.fns[frame.fn_id.0 as usize].code;
            if frame.pc >= code.len() {
                panic!("function {:?} fell off its code without Ret", fn_id);
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
                Op::DivI64 { dst, a, b } => {
                    let va = read_i64_at(&self.arena, base + a as usize);
                    let vb = read_i64_at(&self.arena, base + b as usize);
                    let value = if vb == 0 { 0 } else { va.wrapping_div(vb) };
                    write_i64_at(&mut self.arena, base + dst as usize, value);
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::SubI64 { dst, a, b } => {
                    let va = read_i64_at(&self.arena, base + a as usize);
                    let vb = read_i64_at(&self.arena, base + b as usize);
                    write_i64_at(&mut self.arena, base + dst as usize, va.wrapping_sub(vb));
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::CopyI64 { dst, src } => {
                    let v = read_i64_at(&self.arena, base + src as usize);
                    write_i64_at(&mut self.arena, base + dst as usize, v);
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::EqI64 { dst, a, b } => {
                    let va = read_i64_at(&self.arena, base + a as usize);
                    let vb = read_i64_at(&self.arena, base + b as usize);
                    write_i64_at(&mut self.arena, base + dst as usize, i64::from(va == vb));
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::NeI64 { dst, a, b } => {
                    let va = read_i64_at(&self.arena, base + a as usize);
                    let vb = read_i64_at(&self.arena, base + b as usize);
                    write_i64_at(&mut self.arena, base + dst as usize, i64::from(va != vb));
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::LtI64 { dst, a, b } => {
                    let va = read_i64_at(&self.arena, base + a as usize);
                    let vb = read_i64_at(&self.arena, base + b as usize);
                    write_i64_at(&mut self.arena, base + dst as usize, i64::from(va < vb));
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::LeI64 { dst, a, b } => {
                    let va = read_i64_at(&self.arena, base + a as usize);
                    let vb = read_i64_at(&self.arena, base + b as usize);
                    write_i64_at(&mut self.arena, base + dst as usize, i64::from(va <= vb));
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::GtI64 { dst, a, b } => {
                    let va = read_i64_at(&self.arena, base + a as usize);
                    let vb = read_i64_at(&self.arena, base + b as usize);
                    write_i64_at(&mut self.arena, base + dst as usize, i64::from(va > vb));
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::GeI64 { dst, a, b } => {
                    let va = read_i64_at(&self.arena, base + a as usize);
                    let vb = read_i64_at(&self.arena, base + b as usize);
                    write_i64_at(&mut self.arena, base + dst as usize, i64::from(va >= vb));
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::Jump { target } => {
                    self.frames.last_mut().expect("frame").pc = target as usize;
                }
                Op::JumpIfZero { value, target } => {
                    let v = read_i64_at(&self.arena, base + value as usize);
                    let frame = self.frames.last_mut().expect("frame");
                    if v == 0 {
                        frame.pc = target as usize;
                    } else {
                        frame.pc += 1;
                    }
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
                        self.arena.copy_within(src..src + copy.size as usize, dst);
                    }
                    self.frames.push(FrameRecord {
                        fn_id: callee,
                        base: callee_frame,
                        pc: 0,
                        ret_to: Some(base + ret as usize),
                    });
                    self.trace.push(TaskEvent::FrameEntered(callee));
                }
                Op::CallIndirect { callee, args, ret } => {
                    let callee = FnId(
                        u32::try_from(read_i64_at(&self.arena, base + callee as usize))
                            .expect("indirect callee is a non-negative local function id"),
                    );
                    self.frames.last_mut().expect("frame").pc += 1;
                    let callee_frame = self.alloc_frame(program.fns[callee.0 as usize].frame);
                    for copy in &args {
                        let src = base + copy.src as usize;
                        let dst = callee_frame + copy.dst as usize;
                        self.arena.copy_within(src..src + copy.size as usize, dst);
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
                Op::LoadIndexedI64 {
                    dst,
                    base: arr,
                    index,
                    stride,
                } => {
                    let ix = read_i64_at(&self.arena, base + index as usize);
                    let at = base + arr as usize + ix as usize * stride as usize;
                    let v = read_i64_at(&self.arena, at);
                    write_i64_at(&mut self.arena, base + dst as usize, v);
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::StoreIndexedI64 {
                    base: arr,
                    index,
                    stride,
                    src,
                } => {
                    let ix = read_i64_at(&self.arena, base + index as usize);
                    let v = read_i64_at(&self.arena, base + src as usize);
                    let at = base + arr as usize + ix as usize * stride as usize;
                    write_i64_at(&mut self.arena, at, v);
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::LoadArrayWord {
                    dst,
                    present,
                    array,
                    index,
                    elem_schema_ref,
                } => {
                    let array = read_i64_at(&self.arena, base + array as usize);
                    let index = read_i64_at(&self.arena, base + index as usize);
                    let (ok, value) =
                        load_array_word(value_memories, array, index, elem_schema_ref);
                    write_i64_at(&mut self.arena, base + dst as usize, value);
                    write_i64_at(&mut self.arena, base + present as usize, i64::from(ok));
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::CompareValueBytes { dst, a, b } => {
                    let a = read_i64_at(&self.arena, base + a as usize);
                    let b = read_i64_at(&self.arena, base + b as usize);
                    let ordering = compare_value_bytes(value_memories, a, b);
                    write_i64_at(&mut self.arena, base + dst as usize, ordering);
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::Trace { id } => {
                    if self.mode == TraceMode::Innards {
                        self.trace.push(TaskEvent::Mark(id));
                    }
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::ConstF64 { dst, bits } => {
                    write_i64_at(&mut self.arena, base + dst as usize, bits as i64);
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::AddF64 { dst, a, b } => {
                    let va = f64::from_bits(read_i64_at(&self.arena, base + a as usize) as u64);
                    let vb = f64::from_bits(read_i64_at(&self.arena, base + b as usize) as u64);
                    write_i64_at(
                        &mut self.arena,
                        base + dst as usize,
                        (va + vb).to_bits() as i64,
                    );
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::MulF64 { dst, a, b } => {
                    let va = f64::from_bits(read_i64_at(&self.arena, base + a as usize) as u64);
                    let vb = f64::from_bits(read_i64_at(&self.arena, base + b as usize) as u64);
                    write_i64_at(
                        &mut self.arena,
                        base + dst as usize,
                        (va * vb).to_bits() as i64,
                    );
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::HostCall { host } => {
                    let frame_layout = program.fns[fn_id.0 as usize].frame;
                    let end = base + frame_layout.size;
                    hosts[host as usize](&mut self.arena[base..end]);
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::HostCallYield { host } => {
                    let frame_layout = program.fns[fn_id.0 as usize].frame;
                    let end = base + frame_layout.size;
                    hosts[host as usize](&mut self.arena[base..end]);
                    self.frames.last_mut().expect("frame").pc += 1;
                    return TaskStep::Yielded;
                }
                Op::Await { dst, input } => {
                    let idx = input as usize;
                    if let Some(is_ready) = ready.get_mut(idx)
                        && *is_ready
                    {
                        *is_ready = false;
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

/// One burst of task progress — implemented by both lanes so the
/// executor driver below (and vix's demand driver later) can hold
/// either without caring which.
pub trait Advance {
    fn advance(
        &mut self,
        ready: &mut [bool],
        awaited: &[i64],
        hosts: &mut [HostFn<'_>],
        value_memories: ValueMemories<'_>,
    ) -> TaskStep;
    fn result_bytes(&self) -> &[u8];
}

/// The interpreter lane bundled with its program.
pub struct Running<'p> {
    pub program: &'p Program,
    pub task: Task,
}

impl Advance for Running<'_> {
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

/// TOOTH 3 — the async host boundary: a task driven as a real Rust
/// [`Future`], one input future per await index (an "async host call"
/// IS an await whose input is fed by the host's future — the ruled
/// sync/async distinction from the other side). Depends only on
/// `core::future`; bring any executor. The waker-precision rule from
/// the proven async lane carries over: while parked, a wakeup that
/// didn't ready the BLOCKING input never re-enters the lane.
pub struct TaskExec<'h, A: Advance> {
    lane: A,
    inners: Vec<Pin<Box<dyn Future<Output = i64> + 'h>>>,
    hosts: Vec<BoxedHostFn<'h>>,
    resolved: Vec<bool>,
    ready: Vec<bool>,
    awaited: Vec<i64>,
    parked_on: Option<u32>,
}

impl<'h, A: Advance> TaskExec<'h, A> {
    pub fn new(
        lane: A,
        inners: Vec<Pin<Box<dyn Future<Output = i64> + 'h>>>,
        hosts: Vec<BoxedHostFn<'h>>,
    ) -> Self {
        let n = inners.len();
        TaskExec {
            lane,
            inners,
            hosts,
            resolved: vec![false; n],
            ready: vec![false; n],
            awaited: vec![0; n],
            parked_on: None,
        }
    }

    pub fn lane(&self) -> &A {
        &self.lane
    }
}

impl<A: Advance + Unpin> Future for TaskExec<'_, A> {
    type Output = Vec<u8>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Vec<u8>> {
        let this = &mut *self;

        // Drive EVERY unresolved input; independent awaits make
        // progress concurrently.
        for i in 0..this.inners.len() {
            if !this.resolved[i]
                && let Poll::Ready(value) = this.inners[i].as_mut().poll(cx)
            {
                this.awaited[i] = value;
                this.ready[i] = true;
                this.resolved[i] = true;
            }
        }

        // Parked and the blocking input still isn't ready: don't
        // re-enter the lane.
        if let Some(i) = this.parked_on
            && !this.ready[i as usize]
        {
            return Poll::Pending;
        }

        let mut host_refs: Vec<HostFn<'_>> = this
            .hosts
            .iter_mut()
            .map(|h| h.as_mut() as HostFn<'_>)
            .collect();
        loop {
            match this.lane.advance(
                &mut this.ready,
                &this.awaited,
                &mut host_refs,
                ValueMemories::empty(),
            ) {
                TaskStep::Done => return Poll::Ready(this.lane.result_bytes().to_vec()),
                TaskStep::Yielded => {}
                TaskStep::Parked { input } => {
                    this.parked_on = Some(input);
                    return Poll::Pending;
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

fn load_array_word(
    value_memories: ValueMemories<'_>,
    array: i64,
    index: i64,
    elem_schema_ref: i64,
) -> (bool, i64) {
    let (handle, memories) = if array < 0 {
        let Some(handle) = (-1i64).checked_sub(array) else {
            return (false, 0);
        };
        let Ok(handle) = usize::try_from(handle) else {
            return (false, 0);
        };
        (handle, value_memories.molten)
    } else {
        let Ok(handle) = usize::try_from(array) else {
            return (false, 0);
        };
        (handle, value_memories.store)
    };
    let Some(memory) = memories.get(handle).copied() else {
        return (false, 0);
    };
    if memory.ptr.is_null() || memory.len < 24 || index < 0 {
        return (false, 0);
    }
    let bytes = unsafe { core::slice::from_raw_parts(memory.ptr, memory.len) };
    if read_i64_at(bytes, 0) != 0 || read_i64_at(bytes, 8) != elem_schema_ref {
        return (false, 0);
    }
    let Ok(count) = usize::try_from(read_i64_at(bytes, 16)) else {
        return (false, 0);
    };
    let Some(expected) = count.checked_mul(8).and_then(|n| 24usize.checked_add(n)) else {
        return (false, 0);
    };
    if bytes.len() != expected {
        return (false, 0);
    }
    let index = usize::try_from(index).expect("nonnegative index checked");
    if index >= count {
        return (false, 0);
    }
    (true, read_i64_at(bytes, 24 + index * 8))
}

fn compare_value_bytes(value_memories: ValueMemories<'_>, a: i64, b: i64) -> i64 {
    if a == b {
        return 1;
    }
    let a = value_bytes(value_memories, a)
        .expect("CompareValueBytes left operand must be a resident value handle");
    let b = value_bytes(value_memories, b)
        .expect("CompareValueBytes right operand must be a resident value handle");
    match a.cmp(b) {
        core::cmp::Ordering::Less => 0,
        core::cmp::Ordering::Equal => 1,
        core::cmp::Ordering::Greater => 2,
    }
}

fn value_bytes(value_memories: ValueMemories<'_>, handle: i64) -> Option<&[u8]> {
    let (handle, memories) = if handle < 0 {
        let handle = (-1i64).checked_sub(handle)?;
        (usize::try_from(handle).ok()?, value_memories.molten)
    } else {
        (usize::try_from(handle).ok()?, value_memories.store)
    };
    let memory = memories.get(handle).copied()?;
    if memory.ptr.is_null() {
        return None;
    }
    Some(unsafe { core::slice::from_raw_parts(memory.ptr, memory.len) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mem::Access;
    use crate::mem::declared::{declared_struct, i64_};

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
                mul_plus_x(),
            ],
        };
        let mut task = Task::spawn(&program, FnId(0));
        assert_eq!(task.run(&program, &mut [], &[]), TaskStep::Done);
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
        let mut ready = [false];

        assert_eq!(
            task.run(&program, &mut ready, &[0]),
            TaskStep::Parked { input: 0 }
        );
        assert_eq!(task.depth(), 2, "both frames live while parked");
        assert!(task.trace.contains(&TaskEvent::Parked { input: 0 }));

        // The task struct IS the suspended state; nothing else exists.
        ready[0] = true;
        assert_eq!(task.run(&program, &mut ready, &[21]), TaskStep::Done);
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
                code: vec![Op::Await { dst: 0, input: 0 }, Op::Ret { src: 0, size: 8 }],
            }],
        };
        let mut task = Task::spawn(&program, FnId(0));
        let mut ready = [true];
        assert_eq!(task.run(&program, &mut ready, &[42]), TaskStep::Done);
        assert_eq!(task.result_i64(), 42);
        assert!(
            !task
                .trace
                .iter()
                .any(|e| matches!(e, TaskEvent::Parked { .. }))
        );
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
                                ArgCopy {
                                    src: 0,
                                    dst: x,
                                    size: 8,
                                },
                                ArgCopy {
                                    src: 8,
                                    dst: y,
                                    size: 8,
                                },
                            ],
                            ret: 16,
                        },
                        Op::Ret { src: 16, size: 8 },
                    ],
                },
                Fn {
                    frame: frame_desc.layout,
                    code: vec![
                        Op::MulI64 {
                            dst: out,
                            a: x,
                            b: y,
                        },
                        Op::Ret { src: out, size: 8 },
                    ],
                },
            ],
        };
        let mut task = Task::spawn(&program, FnId(0));
        assert_eq!(task.run(&program, &mut [], &[]), TaskStep::Done);
        assert_eq!(task.result_i64(), 54);
    }

    #[test]
    fn inline_composites_pass_by_value_and_survive_parking() {
        // Amos's stress: a 48-byte inline array of six i64s living IN
        // frames (the "stack" that happens to be arena-heap), never
        // boxed. The whole array crosses a call BY VALUE in one
        // ArgCopy; the callee parks on an await with the composite
        // live in BOTH frames; the callee mutates ITS copy; the
        // caller's copy is untouched (value semantics).
        use crate::mem::Access;
        use crate::mem::declared::{array_of, declared_struct, i64_};

        // Caller frame: (header, arr[6], out, idx, val).
        let caller_desc = declared_struct(
            (),
            vec![
                i64_(()),
                array_of((), i64_(()), 6),
                i64_(()),
                i64_(()),
                i64_(()),
            ],
        );
        let Access::Record(caller_rec) = &caller_desc.access else {
            panic!("record");
        };
        let off = |i: usize| u32::try_from(caller_rec.fields[i].offset).unwrap();
        let (header, arr, out, idx, val) = (off(0), off(1), off(2), off(3), off(4));

        // Callee frame: (arr[6], ix, a, b, sum).
        let callee_desc = declared_struct(
            (),
            vec![
                array_of((), i64_(()), 6),
                i64_(()),
                i64_(()),
                i64_(()),
                i64_(()),
            ],
        );
        let Access::Record(callee_rec) = &callee_desc.access else {
            panic!("record");
        };
        let coff = |i: usize| u32::try_from(callee_rec.fields[i].offset).unwrap();
        let (c_arr, c_ix, c_a, c_b, c_sum) = (coff(0), coff(1), coff(2), coff(3), coff(4));
        assert_eq!(
            callee_rec.fields[0].descriptor.layout.size, 48,
            "inline, unboxed"
        );

        let mut caller_code = vec![Op::ConstI64 {
            dst: header,
            value: 7,
        }];
        // Fill arr[k] = 10*(k+1) through the dynamic-index op.
        for k in 0..6i64 {
            caller_code.push(Op::ConstI64 { dst: idx, value: k });
            caller_code.push(Op::ConstI64 {
                dst: val,
                value: 10 * (k + 1),
            });
            caller_code.push(Op::StoreIndexedI64 {
                base: arr,
                index: idx,
                stride: 8,
                src: val,
            });
        }
        caller_code.push(Op::Call {
            callee: FnId(1),
            // ONE copy moves the whole 48-byte composite by value.
            args: vec![ArgCopy {
                src: arr,
                dst: c_arr,
                size: 48,
            }],
            ret: out,
        });
        // Prove the caller's copy survived the callee's mutation:
        // reload own arr[2] (callee overwrites its own arr[2] with 999).
        caller_code.push(Op::ConstI64 { dst: idx, value: 2 });
        caller_code.push(Op::LoadIndexedI64 {
            dst: val,
            base: arr,
            index: idx,
            stride: 8,
        });
        caller_code.push(Op::AddI64 {
            dst: out,
            a: out,
            b: val,
        });
        caller_code.push(Op::Ret { src: out, size: 8 });

        let callee_code = vec![
            // Park FIRST — the 48-byte composite is live in both
            // frames across the suspension.
            Op::Await {
                dst: c_ix,
                input: 0,
            },
            Op::LoadIndexedI64 {
                dst: c_a,
                base: c_arr,
                index: c_ix,
                stride: 8,
            },
            Op::ConstI64 {
                dst: c_sum,
                value: 1,
            },
            Op::AddI64 {
                dst: c_ix,
                a: c_ix,
                b: c_sum,
            },
            Op::LoadIndexedI64 {
                dst: c_b,
                base: c_arr,
                index: c_ix,
                stride: 8,
            },
            Op::AddI64 {
                dst: c_sum,
                a: c_a,
                b: c_b,
            },
            // Mutate OUR copy: arr[ix] = 999 (value semantics check).
            Op::ConstI64 {
                dst: c_a,
                value: 999,
            },
            Op::StoreIndexedI64 {
                base: c_arr,
                index: c_ix,
                stride: 8,
                src: c_a,
            },
            Op::Ret {
                src: c_sum,
                size: 8,
            },
        ];

        let program = Program {
            fns: vec![
                Fn {
                    frame: caller_desc.layout,
                    code: caller_code,
                },
                Fn {
                    frame: callee_desc.layout,
                    code: callee_code,
                },
            ],
        };
        let mut task = Task::spawn(&program, FnId(0));
        let mut ready = [false];
        assert_eq!(
            task.run(&program, &mut ready, &[0]),
            TaskStep::Parked { input: 0 }
        );
        assert_eq!(
            task.depth(),
            2,
            "parked with 48-byte composites live in both frames"
        );

        // ix=2: a=arr[2]=30, b=arr[3]=40, sum=70; caller adds its own
        // UNMUTATED arr[2]=30 → 100. (If by-value copying were shared,
        // the callee's 999 would bleed through and this would be 1069.)
        ready[0] = true;
        assert_eq!(task.run(&program, &mut ready, &[2]), TaskStep::Done);
        assert_eq!(task.result_i64(), 100);
    }

    #[test]
    fn composite_returns_flow_through_ret_slots() {
        // A callee builds a 24-byte inline array and returns the WHOLE
        // composite through the caller's designated slot (sret shape);
        // the caller indexes into the returned bytes in place.
        let program = Program {
            fns: vec![
                Fn {
                    // (ret_arr[3] @0, idx @24, out @32)
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
                Fn {
                    // (arr[3] @0, idx @24, val @32)
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
        let mut task = Task::spawn(&program, FnId(0));
        assert_eq!(task.run(&program, &mut [], &[]), TaskStep::Done);
        assert_eq!(
            task.result_i64(),
            6,
            "indexed into the 24-byte returned composite"
        );
    }

    fn later(value: i64, ms: u64) -> Pin<Box<dyn Future<Output = i64>>> {
        Box::pin(async move {
            tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
            value
        })
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn tasks_await_real_futures_across_call_frames() {
        // outer calls callee; callee awaits TWO real futures (late #0,
        // early #1) and combines them with a frame local. The demand
        // driver shape vix will use, in miniature.
        let program = Program {
            fns: vec![
                Fn {
                    frame: frame_of_i64s(2),
                    code: vec![
                        Op::ConstI64 {
                            dst: 0,
                            value: 1000,
                        },
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
                    frame: frame_of_i64s(3),
                    code: vec![
                        Op::Await { dst: 0, input: 0 },
                        Op::Await { dst: 8, input: 1 },
                        Op::MulI64 {
                            dst: 16,
                            a: 0,
                            b: 8,
                        },
                        Op::Ret { src: 16, size: 8 },
                    ],
                },
            ],
        };
        let running = Running {
            program: &program,
            task: Task::spawn(&program, FnId(0)),
        };
        let exec = TaskExec::new(running, vec![later(6, 60), later(7, 20)], vec![]);
        let result = exec.await;
        assert_eq!(
            i64::from_le_bytes(result[..8].try_into().unwrap()),
            6 * 7 + 1000
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn external_wakes_resume_parked_tasks() {
        // The async-host shape: a oneshot fed by another tokio task —
        // an external event wakes the parked task through the ordinary
        // waker path, and sync host calls coexist in the same run.
        let program = Program {
            fns: vec![Fn {
                frame: frame_of_i64s(2),
                code: vec![
                    Op::Await { dst: 0, input: 0 },
                    Op::HostCall { host: 0 },
                    Op::Ret { src: 8, size: 8 },
                ],
            }],
        };
        let (tx, rx) = tokio::sync::oneshot::channel::<i64>();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
            tx.send(21).unwrap();
        });
        let input: Pin<Box<dyn Future<Output = i64>>> =
            Box::pin(async move { rx.await.expect("sender lives") });
        let host: BoxedHostFn = Box::new(|frame: &mut [u8]| {
            let v = i64::from_le_bytes(frame[0..8].try_into().unwrap());
            frame[8..16].copy_from_slice(&(v * 2).to_le_bytes());
        });
        let running = Running {
            program: &program,
            task: Task::spawn(&program, FnId(0)),
        };
        let result = TaskExec::new(running, vec![input], vec![host]).await;
        assert_eq!(i64::from_le_bytes(result[..8].try_into().unwrap()), 42);
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
                    args: vec![ArgCopy {
                        src: 0,
                        dst: 0,
                        size: 8,
                    }],
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
                    args: vec![ArgCopy {
                        src: 0,
                        dst: 0,
                        size: 8,
                    }],
                    ret: 8,
                },
                Op::Ret { src: 8, size: 8 },
            ],
        };
        let program = Program {
            fns: vec![root, mid, leaf],
        };
        let mut task = Task::spawn(&program, FnId(0));
        assert_eq!(task.run(&program, &mut [], &[]), TaskStep::Done);
        // leaf: 10+1=11; mid: 11+10=21.
        assert_eq!(task.result_i64(), 21);
        assert_eq!(task.depth(), 0);
    }

    #[test]
    fn direct_recursion_uses_task_frames_not_the_rust_stack() {
        let countdown = Fn {
            frame: frame_of_i64s(6),
            code: vec![
                Op::ConstI64 { dst: 8, value: 0 },
                Op::EqI64 {
                    dst: 24,
                    a: 0,
                    b: 8,
                },
                Op::JumpIfZero {
                    value: 24,
                    target: 4,
                },
                Op::Ret { src: 8, size: 8 },
                Op::ConstI64 { dst: 16, value: 1 },
                Op::SubI64 {
                    dst: 32,
                    a: 0,
                    b: 16,
                },
                Op::Call {
                    callee: FnId(0),
                    args: vec![ArgCopy {
                        src: 32,
                        dst: 0,
                        size: 8,
                    }],
                    ret: 40,
                },
                Op::Ret { src: 40, size: 8 },
            ],
        };
        let program = Program {
            fns: vec![countdown],
        };
        let mut task = Task::spawn_with_mode(&program, FnId(0), TraceMode::Production);
        task.write_i64(0, 100_000);

        assert_eq!(task.run(&program, &mut [], &[]), TaskStep::Done);
        assert_eq!(task.result_i64(), 0);
        assert_eq!(task.depth(), 0);
    }

    #[test]
    fn mutual_recursion_calls_through_recorded_fn_ids() {
        let even = Fn {
            frame: frame_of_i64s(6),
            code: vec![
                Op::ConstI64 { dst: 8, value: 0 },
                Op::EqI64 {
                    dst: 24,
                    a: 0,
                    b: 8,
                },
                Op::JumpIfZero {
                    value: 24,
                    target: 5,
                },
                Op::ConstI64 { dst: 40, value: 1 },
                Op::Ret { src: 40, size: 8 },
                Op::ConstI64 { dst: 16, value: 1 },
                Op::SubI64 {
                    dst: 32,
                    a: 0,
                    b: 16,
                },
                Op::Call {
                    callee: FnId(1),
                    args: vec![ArgCopy {
                        src: 32,
                        dst: 0,
                        size: 8,
                    }],
                    ret: 40,
                },
                Op::Ret { src: 40, size: 8 },
            ],
        };
        let odd = Fn {
            frame: frame_of_i64s(6),
            code: vec![
                Op::ConstI64 { dst: 8, value: 0 },
                Op::EqI64 {
                    dst: 24,
                    a: 0,
                    b: 8,
                },
                Op::JumpIfZero {
                    value: 24,
                    target: 5,
                },
                Op::ConstI64 { dst: 40, value: 0 },
                Op::Ret { src: 40, size: 8 },
                Op::ConstI64 { dst: 16, value: 1 },
                Op::SubI64 {
                    dst: 32,
                    a: 0,
                    b: 16,
                },
                Op::Call {
                    callee: FnId(0),
                    args: vec![ArgCopy {
                        src: 32,
                        dst: 0,
                        size: 8,
                    }],
                    ret: 40,
                },
                Op::Ret { src: 40, size: 8 },
            ],
        };
        let program = Program {
            fns: vec![even, odd],
        };
        let mut task = Task::spawn(&program, FnId(0));
        task.write_i64(0, 101);

        assert_eq!(task.run(&program, &mut [], &[]), TaskStep::Done);
        assert_eq!(task.result_i64(), 0);
    }
}
