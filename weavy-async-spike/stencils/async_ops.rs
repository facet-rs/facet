//! Copy-and-patch stencils with a SUSPEND POINT — the weavy-async crux.
//!
//! The insight: a copy-and-patch chain already keeps its state OFF the C stack
//! (`Ctx` is an explicit struct: prog + operand stack pointer + host cells),
//! and the `become` (guaranteed-tail-call) discipline means no stencil holds
//! live C-stack state across its continuation. So the whole chain runs in ONE
//! stack frame (the driver's call), and any stencil can SUSPEND by simply
//! returning up — losing nothing — instead of tail-calling through. Resume =
//! re-enter at the awaited stencil's offset. This is the two-successor guard
//! stencil with the slow path repurposed from "deopt to interpreter" to
//! "yield Pending to the async executor".
//!
//! build.rs compiles this with `rustc --emit=obj` and extracts each op's
//! machine code + its `weavy_cont` relocation, exactly like the intop lane.

#![cfg_attr(tailcall, feature(explicit_tail_calls))]
#![allow(clippy::missing_safety_doc)]
#![allow(incomplete_features)]

/// Threaded state — MUST match `Ctx` in src/lib.rs (repr(C), same order).
#[repr(C)]
pub struct Ctx {
    /// Immediate stream (push reads from here).
    pub prog: *const u64,
    /// Operand stack pointer (next free slot).
    pub sp: *mut i64,
    /// Host readiness cell: 0 = the awaited value is not ready, else ready.
    pub ready: *const i64,
    /// The awaited value (valid when `*ready != 0`).
    pub awaited: *const i64,
    /// The stencil writes 1 here when it suspends — the driver reads it to
    /// distinguish "chain finished" from "chain parked on an await".
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

/// AWAIT: if the host cell says ready, push the awaited value and continue the
/// FAST chain; else flag suspension and RETURN — unwinding to the driver,
/// which re-enters HERE (this stencil's offset) when the value lands. The
/// entire mechanism is: continue vs. return. That's async.
#[no_mangle]
pub unsafe extern "C" fn weavy_await(cx: *mut Ctx) {
    let c = &mut *cx;
    if *c.ready != 0 {
        *c.sp = *c.awaited;
        c.sp = c.sp.add(1);
        cont!(cx);
    } else {
        *c.suspended = 1;
        // Return up the (single, driver-owned) stack frame. State lives in
        // Ctx, so nothing is lost; resume re-enters this stencil.
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

/// End of chain: return to the driver with the result on the stack.
#[no_mangle]
pub unsafe extern "C" fn weavy_done(_cx: *mut Ctx) {}
