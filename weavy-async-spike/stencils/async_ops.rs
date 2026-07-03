//! Copy-and-patch stencils with MULTIPLE SUSPEND POINTS — a real resumable
//! state machine.
//!
//! The insight (proven in phase 1): a copy-and-patch chain keeps its state OFF
//! the C stack (`Ctx` is an explicit struct), and `become` (guaranteed tail
//! calls) means no stencil holds live C-stack state across its continuation.
//! So the whole chain runs in ONE driver-owned stack frame; a stencil SUSPENDS
//! by returning up (losing nothing) and RESUMES by re-entering at a saved
//! offset.
//!
//! Phase 2 generalizes to N awaits. A stencil can't know its own address, so
//! the compiler BAKES each await's resume offset (its own chain offset) and
//! its await index into the immediate stream. On suspend the await writes both
//! into `Ctx` (the driver re-enters at `resume`, and `await_index` tells the
//! driver which future parked). On ready it consumes those two immediates and
//! continues. Readiness/values are HOST ARRAYS indexed by await index, so
//! independent awaits resolve concurrently even though the chain visits them
//! in order.
//!
//! build.rs compiles this with `rustc --emit=obj` and extracts each op's
//! machine code + its `weavy_cont` relocation, like the intop lane.

#![cfg_attr(tailcall, feature(explicit_tail_calls))]
#![allow(clippy::missing_safety_doc)]
#![allow(incomplete_features)]

/// Threaded state — MUST match `Ctx` in src/lib.rs (repr(C), same order).
#[repr(C)]
pub struct Ctx {
    /// Immediate stream (push reads values; await reads [resume_off, index]).
    pub prog: *const u64,
    /// Operand stack pointer (next free slot).
    pub sp: *mut i64,
    /// Host readiness array: `ready[i] != 0` ⇒ await #i's value is present.
    pub ready: *const i64,
    /// Host value array, indexed by await index.
    pub awaited: *const i64,
    /// On suspend, the await writes ITS OWN chain offset here — the driver
    /// re-enters exactly this stencil on resume.
    pub resume: *mut u64,
    /// On suspend, the await writes its index here — the driver learns which
    /// future parked the chain.
    pub await_index: *mut u64,
    /// Set to 1 on suspend; the driver reads it to tell "finished" from
    /// "parked on an await".
    pub suspended: *mut i64,
}

extern "C" {
    /// The continuation hole (patched to the next stencil, or DONE).
    fn weavy_cont(cx: *mut Ctx);
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

/// Push one immediate onto the operand stack.
#[no_mangle]
pub unsafe extern "C" fn weavy_push(cx: *mut Ctx) {
    let c = &mut *cx;
    *c.sp = *c.prog as i64;
    c.sp = c.sp.add(1);
    c.prog = c.prog.add(1);
    cont!(cx);
}

/// AWAIT #index. Reads two immediates WITHOUT advancing on the pending path,
/// so a resume re-reads the same [resume_off, index] — the state machine is
/// idempotent per suspend point.
#[no_mangle]
pub unsafe extern "C" fn weavy_await(cx: *mut Ctx) {
    let c = &mut *cx;
    let resume_off = *c.prog;
    let index = *c.prog.add(1) as usize;
    if *c.ready.add(index) != 0 {
        // Ready: consume the two immediates, push the value, continue fast.
        c.prog = c.prog.add(2);
        *c.sp = *c.awaited.add(index);
        c.sp = c.sp.add(1);
        cont!(cx);
    } else {
        // Pending: record where/what parked us; leave prog untouched so the
        // resume re-enters cleanly. Then return up to the driver.
        *c.resume = resume_off;
        *c.await_index = index as u64;
        *c.suspended = 1;
    }
}

/// Pop two, push their sum.
#[no_mangle]
pub unsafe extern "C" fn weavy_add(cx: *mut Ctx) {
    let c = &mut *cx;
    let b = *c.sp.sub(1);
    let a = *c.sp.sub(2);
    c.sp = c.sp.sub(1);
    *c.sp.sub(1) = a + b;
    cont!(cx);
}

/// Pop two, push their product (a second op, to prove chains of real work
/// survive multiple suspends unchanged).
#[no_mangle]
pub unsafe extern "C" fn weavy_mul(cx: *mut Ctx) {
    let c = &mut *cx;
    let b = *c.sp.sub(1);
    let a = *c.sp.sub(2);
    c.sp = c.sp.sub(1);
    *c.sp.sub(1) = a * b;
    cont!(cx);
}

/// End of chain: return to the driver with the result on the stack.
#[no_mangle]
pub unsafe extern "C" fn weavy_done(_cx: *mut Ctx) {}
