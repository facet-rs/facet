//! Copy-and-patch stencils for the unboxed integer eval lane, in Rust.
//!
//! `build.rs` compiles this with `rustc --emit=obj` and extracts each stencil's machine code
//! plus its `weavy_cont` relocation (the continuation hole) by symbol — exactly how weavy's own
//! `hostcall.rs` and phon-jit's `stencils.rs` work. Each op's BODY is compiled native code; the
//! only external reference is the tail call to `weavy_cont`, whose `BRANCH26` reloc is patched
//! at assembly time to chain the copied stencils. Immediates ride in `Ctx.prog`.
//!
//! This is real copy-and-patch: no host-call, no indirect dispatch, no interpreter loop — the
//! add is a native add.
//!
//! Under `--cfg tailcall` (enabled on stable via `RUSTC_BOOTSTRAP=1`, no nightly toolchain
//! needed), the continuation is a GUARANTEED tail call (`become`) — a compile error if it
//! can't be, so a future stencil that grows a destructor can't silently regress to a `bl`
//! call chain. At `-O` LLVM already TCOs the plain call to the same `b`, so the shipped bytes
//! are identical; `become` is the guard, not a speedup.

#![cfg_attr(tailcall, feature(explicit_tail_calls))]
#![allow(clippy::missing_safety_doc)]
#![allow(incomplete_features)]

/// Tail-call the continuation: guaranteed `become` under `--cfg tailcall`, else a plain call
/// that `-O` tail-call-optimizes to the same branch.
macro_rules! cont {
    ($f:ident, $cx:ident) => {{
        #[cfg(tailcall)]
        {
            become $f($cx);
        }
        #[cfg(not(tailcall))]
        {
            $f($cx);
        }
    }};
}

/// Threaded state: the immediate stream and the i64 operand stack pointer (next free slot).
#[repr(C)]
pub struct Ctx {
    pub prog: *const u64,
    pub sp: *mut i64,
}

extern "C" {
    /// The continuation hole. Tail-calling it emits the `BRANCH26` relocation build.rs extracts
    /// and the runtime patches to the next stencil (or `done`).
    fn weavy_cont(cx: *mut Ctx);
}

/// Push one immediate (read from `prog`) onto the stack.
#[no_mangle]
pub unsafe extern "C" fn weavy_intop_push(cx: *mut Ctx) {
    let c = &mut *cx;
    let v = *(c.prog as *const i64);
    c.prog = c.prog.add(1);
    *c.sp = v;
    c.sp = c.sp.add(1);
    cont!(weavy_cont, cx);
}

/// `a + b` (native add, wrapping — no panic path under `panic=abort`).
#[no_mangle]
pub unsafe extern "C" fn weavy_intop_add(cx: *mut Ctx) {
    let c = &mut *cx;
    let b = *c.sp.sub(1);
    let a = *c.sp.sub(2);
    *c.sp.sub(2) = a.wrapping_add(b);
    c.sp = c.sp.sub(1);
    cont!(weavy_cont, cx);
}

/// `a - b`.
#[no_mangle]
pub unsafe extern "C" fn weavy_intop_sub(cx: *mut Ctx) {
    let c = &mut *cx;
    let b = *c.sp.sub(1);
    let a = *c.sp.sub(2);
    *c.sp.sub(2) = a.wrapping_sub(b);
    c.sp = c.sp.sub(1);
    cont!(weavy_cont, cx);
}

/// `a * b`.
#[no_mangle]
pub unsafe extern "C" fn weavy_intop_mul(cx: *mut Ctx) {
    let c = &mut *cx;
    let b = *c.sp.sub(1);
    let a = *c.sp.sub(2);
    *c.sp.sub(2) = a.wrapping_mul(b);
    c.sp = c.sp.sub(1);
    cont!(weavy_cont, cx);
}

// --- Float lane: same i64 stack, values stored as f64 bits (push reuses `weavy_intop_push`
//     with f64-bit immediates; only the arithmetic bitcasts). -----------------------------

/// `a + b` as f64 (bitcast in/out of the shared stack).
#[no_mangle]
pub unsafe extern "C" fn weavy_intop_fadd(cx: *mut Ctx) {
    let c = &mut *cx;
    let b = f64::from_bits(*c.sp.sub(1) as u64);
    let a = f64::from_bits(*c.sp.sub(2) as u64);
    *c.sp.sub(2) = (a + b).to_bits() as i64;
    c.sp = c.sp.sub(1);
    cont!(weavy_cont, cx);
}

/// `a - b` as f64.
#[no_mangle]
pub unsafe extern "C" fn weavy_intop_fsub(cx: *mut Ctx) {
    let c = &mut *cx;
    let b = f64::from_bits(*c.sp.sub(1) as u64);
    let a = f64::from_bits(*c.sp.sub(2) as u64);
    *c.sp.sub(2) = (a - b).to_bits() as i64;
    c.sp = c.sp.sub(1);
    cont!(weavy_cont, cx);
}

/// `a * b` as f64.
#[no_mangle]
pub unsafe extern "C" fn weavy_intop_fmul(cx: *mut Ctx) {
    let c = &mut *cx;
    let b = f64::from_bits(*c.sp.sub(1) as u64);
    let a = f64::from_bits(*c.sp.sub(2) as u64);
    *c.sp.sub(2) = (a * b).to_bits() as i64;
    c.sp = c.sp.sub(1);
    cont!(weavy_cont, cx);
}

/// Terminal: a lone `ret`.
#[no_mangle]
pub unsafe extern "C" fn weavy_intop_done(_cx: *mut Ctx) {}
