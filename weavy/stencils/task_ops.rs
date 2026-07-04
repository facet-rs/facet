//! Task-lane copy-and-patch stencils: typed three-address ops over
//! FRAME offsets (tooth 2 of the substrate — frames as declared
//! records in a per-task arena; see the ruled ABI in the vixen repo,
//! docs/design/tooth-2-frames-abi.md).
//!
//! No operand stack and no tags: every op addresses the current frame
//! at statically known byte offsets carried in the immediate stream —
//! the await-spill rule holds by construction because values are
//! always frame-resident (constitution A6: typed instructions over
//! untagged operands).
//!
//! Calls and returns EXIT to the driver in this slice (the driver owns
//! frame allocation — tested logic — and trampolines into the callee's
//! chain). Direct-threaded cross-chain calls are a later optimization;
//! the ABI does not change when they arrive, because the frame layout
//! and the immediate vocabulary stay identical.
//!
//! build.rs compiles this with `rustc --emit=obj` (guaranteed tail
//! calls via `become` where available) and extracts each op's machine
//! code plus its `weavy_cont` relocation, exactly like the async lane.

#![cfg_attr(tailcall, feature(explicit_tail_calls))]
#![allow(clippy::missing_safety_doc)]
#![allow(incomplete_features)]

/// Threaded state — MUST match `Ctx` in src/jit/task_lane.rs
/// (repr(C), same field order).
#[repr(C)]
pub struct Ctx {
    /// Immediate stream (frame offsets, values, await descriptors).
    pub prog: *const u64,
    /// Current frame base. Stable for the duration of one chain entry:
    /// the driver performs all allocation between entries.
    pub frame: *mut u8,
    /// Host readiness array: `ready[i] != 0` ⇒ await #i's value is present.
    pub ready: *const i64,
    /// Host value array, indexed by await index.
    pub awaited: *const i64,
    /// On any driver exit (park/call/ret), the chain offset to re-enter.
    pub resume: *mut u64,
    /// On park, which await parked the task.
    pub await_index: *mut u64,
    /// Exit code: 0 = chain fell through (bug — RET is mandatory),
    /// 1 = parked on an await, 2 = call (driver enters callee),
    /// 3 = ret (driver pops the frame), 4 = sync host call (driver
    /// invokes the host over the frame, re-enters at the continuation).
    pub exit: *mut i64,
}

extern "C" {
    /// The continuation hole (patched to the next stencil, or DONE).
    fn weavy_cont(cx: *mut Ctx);
    /// Conditional continuation patched to the zero/taken target.
    fn weavy_zero(cx: *mut Ctx);
    /// Conditional continuation patched to the nonzero/fallthrough target.
    fn weavy_nonzero(cx: *mut Ctx);
}

macro_rules! cont {
    ($cx:ident) => {{
        #[cfg(tailcall)]
        {
            become weavy_cont($cx);
        }
        #[cfg(not(tailcall))]
        {
            weavy_cont($cx);
        }
    }};
}

macro_rules! branch_to {
    ($target:ident, $cx:ident) => {{
        #[cfg(tailcall)]
        {
            become $target($cx);
        }
        #[cfg(not(tailcall))]
        {
            $target($cx);
        }
    }};
}

#[inline(always)]
unsafe fn read_i64(frame: *mut u8, off: u64) -> i64 {
    (frame.add(off as usize) as *const i64).read_unaligned()
}

#[inline(always)]
unsafe fn write_i64(frame: *mut u8, off: u64, value: i64) {
    (frame.add(off as usize) as *mut i64).write_unaligned(value);
}

/// `frame[dst] = value` — immediates: [dst, value].
#[no_mangle]
pub unsafe extern "C" fn weavy_task_const(cx: *mut Ctx) {
    let c = &mut *cx;
    let dst = *c.prog;
    let value = *c.prog.add(1) as i64;
    c.prog = c.prog.add(2);
    write_i64(c.frame, dst, value);
    cont!(cx);
}

/// `frame[dst] = frame[a] + frame[b]` — immediates: [dst, a, b].
#[no_mangle]
pub unsafe extern "C" fn weavy_task_add(cx: *mut Ctx) {
    let c = &mut *cx;
    let dst = *c.prog;
    let a = *c.prog.add(1);
    let b = *c.prog.add(2);
    c.prog = c.prog.add(3);
    write_i64(c.frame, dst, read_i64(c.frame, a).wrapping_add(read_i64(c.frame, b)));
    cont!(cx);
}

/// `frame[dst] = frame[a] * frame[b]` — immediates: [dst, a, b].
#[no_mangle]
pub unsafe extern "C" fn weavy_task_mul(cx: *mut Ctx) {
    let c = &mut *cx;
    let dst = *c.prog;
    let a = *c.prog.add(1);
    let b = *c.prog.add(2);
    c.prog = c.prog.add(3);
    write_i64(c.frame, dst, read_i64(c.frame, a).wrapping_mul(read_i64(c.frame, b)));
    cont!(cx);
}

/// `frame[dst] = frame[a] - frame[b]` (i64, wrapping) — immediates: [dst, a, b].
#[no_mangle]
pub unsafe extern "C" fn weavy_task_sub(cx: *mut Ctx) {
    let c = &mut *cx;
    let dst = *c.prog;
    let a = *c.prog.add(1);
    let b = *c.prog.add(2);
    c.prog = c.prog.add(3);
    write_i64(c.frame, dst, read_i64(c.frame, a).wrapping_sub(read_i64(c.frame, b)));
    cont!(cx);
}

/// `frame[dst] = frame[src]` — immediates: [dst, src].
#[no_mangle]
pub unsafe extern "C" fn weavy_task_copy(cx: *mut Ctx) {
    let c = &mut *cx;
    let dst = *c.prog;
    let src = *c.prog.add(1);
    c.prog = c.prog.add(2);
    let v = read_i64(c.frame, src);
    write_i64(c.frame, dst, v);
    cont!(cx);
}

macro_rules! cmp_i64_stencil {
    ($name:ident, $op:tt) => {
        /// i64 comparison — immediates: [dst, a, b]. Writes 0/1.
        #[no_mangle]
        pub unsafe extern "C" fn $name(cx: *mut Ctx) {
            let c = &mut *cx;
            let dst = *c.prog;
            let a = *c.prog.add(1);
            let b = *c.prog.add(2);
            c.prog = c.prog.add(3);
            let result = i64::from(read_i64(c.frame, a) $op read_i64(c.frame, b));
            write_i64(c.frame, dst, result);
            cont!(cx);
        }
    };
}

cmp_i64_stencil!(weavy_task_eq, ==);
cmp_i64_stencil!(weavy_task_ne, !=);
cmp_i64_stencil!(weavy_task_lt, <);
cmp_i64_stencil!(weavy_task_le, <=);
cmp_i64_stencil!(weavy_task_gt, >);
cmp_i64_stencil!(weavy_task_ge, >=);

/// Unconditional branch — immediates: [target_prog_delta]. The continuation
/// hole is patched to the target stencil instead of the lexical successor.
#[no_mangle]
pub unsafe extern "C" fn weavy_task_jump(cx: *mut Ctx) {
    let c = &mut *cx;
    let delta = *c.prog as i64 as isize;
    c.prog = c.prog.offset(delta);
    cont!(cx);
}

/// Branch to `target` when `frame[value] == 0`, otherwise fall through —
/// immediates: [value, taken_prog_delta, fallthrough_prog_delta]. The two
/// continuation holes are patched separately.
#[no_mangle]
pub unsafe extern "C" fn weavy_task_jump_if_zero(cx: *mut Ctx) {
    let c = &mut *cx;
    let value = *c.prog;
    let taken_delta = *c.prog.add(1) as i64 as isize;
    let fallthrough_delta = *c.prog.add(2) as i64 as isize;
    if read_i64(c.frame, value) == 0 {
        c.prog = c.prog.offset(taken_delta);
        branch_to!(weavy_zero, cx);
    } else {
        c.prog = c.prog.offset(fallthrough_delta);
        branch_to!(weavy_nonzero, cx);
    }
}

/// `frame[dst] = frame[base + frame[index]*stride]` — immediates:
/// [dst, base, index, stride]. Bounds are the checker's obligation.
#[no_mangle]
pub unsafe extern "C" fn weavy_task_load_ix(cx: *mut Ctx) {
    let c = &mut *cx;
    let dst = *c.prog;
    let base = *c.prog.add(1);
    let index = *c.prog.add(2);
    let stride = *c.prog.add(3);
    c.prog = c.prog.add(4);
    let ix = read_i64(c.frame, index) as u64;
    let v = read_i64(c.frame, base + ix * stride);
    write_i64(c.frame, dst, v);
    cont!(cx);
}

/// `frame[base + frame[index]*stride] = frame[src]` — immediates:
/// [base, index, stride, src].
#[no_mangle]
pub unsafe extern "C" fn weavy_task_store_ix(cx: *mut Ctx) {
    let c = &mut *cx;
    let base = *c.prog;
    let index = *c.prog.add(1);
    let stride = *c.prog.add(2);
    let src = *c.prog.add(3);
    c.prog = c.prog.add(4);
    let ix = read_i64(c.frame, index) as u64;
    let v = read_i64(c.frame, src);
    write_i64(c.frame, base + ix * stride, v);
    cont!(cx);
}

/// AWAIT — immediates: [resume_off, index, dst], NOT consumed on the
/// pending path so a resume re-reads the same descriptor (idempotent
/// per suspend point, exactly the proven async-lane protocol).
#[no_mangle]
pub unsafe extern "C" fn weavy_task_await(cx: *mut Ctx) {
    let c = &mut *cx;
    let resume_off = *c.prog;
    let index = *c.prog.add(1) as usize;
    let dst = *c.prog.add(2);
    if *c.ready.add(index) != 0 {
        c.prog = c.prog.add(3);
        write_i64(c.frame, dst, *c.awaited.add(index));
        cont!(cx);
    } else {
        *c.resume = resume_off;
        *c.await_index = index as u64;
        *c.exit = 1;
    }
}

/// CALL SITE — immediates: [resume_off] (the caller's continuation,
/// where the driver re-enters after the callee returns). The call
/// descriptor (callee, arg copies, return slot) lives in the driver's
/// side table keyed by this same resume offset. Exit code 2.
#[no_mangle]
pub unsafe extern "C" fn weavy_task_call(cx: *mut Ctx) {
    let c = &mut *cx;
    let resume_off = *c.prog;
    c.prog = c.prog.add(1);
    *c.resume = resume_off;
    *c.exit = 2;
}

/// RET SITE — immediates: [src, size]. Exit code 3. The `resume` and
/// `await_index` fields double as exit-payload registers here (src
/// and size respectively) — the driver reads them, copies the return
/// bytes into the caller's designated slot, and pops the frame. A
/// function may have several return sites; each carries its own
/// descriptor.
#[no_mangle]
pub unsafe extern "C" fn weavy_task_ret(cx: *mut Ctx) {
    let c = &mut *cx;
    let src = *c.prog;
    let size = *c.prog.add(1);
    *c.resume = src;
    *c.await_index = size;
    *c.exit = 3;
}

/// `frame[dst] = frame[a] + frame[b]` (f64, IEEE) — immediates: [dst, a, b].
#[no_mangle]
pub unsafe extern "C" fn weavy_task_add_f64(cx: *mut Ctx) {
    let c = &mut *cx;
    let dst = *c.prog;
    let a = *c.prog.add(1);
    let b = *c.prog.add(2);
    c.prog = c.prog.add(3);
    let va = f64::from_bits(read_i64(c.frame, a) as u64);
    let vb = f64::from_bits(read_i64(c.frame, b) as u64);
    write_i64(c.frame, dst, (va + vb).to_bits() as i64);
    cont!(cx);
}

/// `frame[dst] = frame[a] * frame[b]` (f64, IEEE) — immediates: [dst, a, b].
#[no_mangle]
pub unsafe extern "C" fn weavy_task_mul_f64(cx: *mut Ctx) {
    let c = &mut *cx;
    let dst = *c.prog;
    let a = *c.prog.add(1);
    let b = *c.prog.add(2);
    c.prog = c.prog.add(3);
    let va = f64::from_bits(read_i64(c.frame, a) as u64);
    let vb = f64::from_bits(read_i64(c.frame, b) as u64);
    write_i64(c.frame, dst, (va * vb).to_bits() as i64);
    cont!(cx);
}

/// SYNC HOST CALL — immediates: [continuation, host_index], consumed
/// before exit (unlike await: a host call always completes, so
/// re-entry happens at the continuation, never here). Exit code 4;
/// `resume` carries the continuation, `await_index` carries the host
/// index. No park path exists on this op by construction — that is
/// the ruled sync/async distinction in machine code.
#[no_mangle]
pub unsafe extern "C" fn weavy_task_hostcall(cx: *mut Ctx) {
    let c = &mut *cx;
    let continuation = *c.prog;
    let host = *c.prog.add(1);
    c.prog = c.prog.add(2);
    *c.resume = continuation;
    *c.await_index = host;
    *c.exit = 4;
}

/// TRACE MARK — immediates: [continuation, id], consumed before exit
/// (a mark always completes). Exit code 5; `resume` carries the
/// continuation, `await_index` carries the id. Only Innards-mode
/// compilation emits this stencil at all — Production strips the op
/// from the chain entirely (zero instructions), which is the
/// unified-trace ruling's "weavy strips by mode" made literal.
#[no_mangle]
pub unsafe extern "C" fn weavy_task_trace(cx: *mut Ctx) {
    let c = &mut *cx;
    let continuation = *c.prog;
    let id = *c.prog.add(1);
    c.prog = c.prog.add(2);
    *c.resume = continuation;
    *c.await_index = id;
    *c.exit = 5;
}

/// End of chain — reaching this is a lowering bug (RET is mandatory);
/// the driver panics on exit code 0.
#[no_mangle]
pub unsafe extern "C" fn weavy_task_done(_cx: *mut Ctx) {}
