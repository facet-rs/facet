//! Async copy-and-patch stencils: suspend points for the demand-driven lane.
//!
//! The insight these encode: a copy-and-patch chain keeps its state OFF the C
//! stack (`Ctx` is an explicit struct — the operand stack is a separate
//! buffer), and the guaranteed-tail-call discipline means no stencil holds
//! live C-stack state across its continuation. So the whole chain runs in one
//! driver-owned frame; a stencil SUSPENDS by returning up (losing nothing) and
//! RESUMES by re-entering at a saved chain offset. This is the two-successor
//! type-speculation GUARD stencil with the slow path repurposed from "deopt to
//! interpreter" to "yield Pending to the async executor".
//!
//! build.rs compiles this with `rustc --emit=obj` (guaranteed tail calls via
//! `become`, so suspension is sound for arbitrary-length chains) and extracts
//! each op's machine code plus its `weavy_cont` relocation.

#![cfg_attr(tailcall, feature(explicit_tail_calls))]
#![allow(clippy::missing_safety_doc)]
#![allow(incomplete_features)]

/// Threaded state — MUST match `Ctx` in src/jit/async.rs (repr(C), same order).
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
pub unsafe extern "C" fn weavy_async_push(cx: *mut Ctx) {
    let prog = (*cx).prog;
    let sp = (*cx).sp;
    *sp = *prog as i64;

    // Keep these as independent scalar stores. On Linux x86_64, LLVM otherwise
    // combines the adjacent `prog`/`sp` updates into an SSE add with a
    // RIP-relative constant-pool relocation, and copied stencils only support
    // continuation relocations.
    core::ptr::write_volatile(core::ptr::addr_of_mut!((*cx).sp), sp.add(1));
    core::ptr::write_volatile(core::ptr::addr_of_mut!((*cx).prog), prog.add(1));
    cont!(cx);
}

/// AWAIT #index — the suspend point. Reads two immediates WITHOUT advancing on
/// the pending path, so a resume re-reads the same [resume_off, index]: the
/// state machine is idempotent per suspend point.
#[no_mangle]
pub unsafe extern "C" fn weavy_async_await(cx: *mut Ctx) {
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
pub unsafe extern "C" fn weavy_async_add(cx: *mut Ctx) {
    let c = &mut *cx;
    let b = *c.sp.sub(1);
    let a = *c.sp.sub(2);
    c.sp = c.sp.sub(1);
    *c.sp.sub(1) = a + b;
    cont!(cx);
}

/// Pop two, push their product.
#[no_mangle]
pub unsafe extern "C" fn weavy_async_mul(cx: *mut Ctx) {
    let c = &mut *cx;
    let b = *c.sp.sub(1);
    let a = *c.sp.sub(2);
    c.sp = c.sp.sub(1);
    *c.sp.sub(1) = a * b;
    cont!(cx);
}

/// End of chain: return to the driver with the result on the stack.
#[no_mangle]
pub unsafe extern "C" fn weavy_async_done(_cx: *mut Ctx) {}
