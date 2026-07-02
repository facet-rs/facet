//! Type-speculation guard stencil — the heart of a speculative JIT.
//!
//! A guard bets that a variable is an integer. If the bet holds it unboxes the value onto the
//! i64 stack and continues the FAST chain (`weavy_cont`); if it misses it flags a deopt and
//! branches to the DEOPT continuation (`weavy_deopt`) — the caller then falls back to the full
//! evaluator. This is a two-successor stencil: `build.rs` extracts BOTH continuation holes
//! (via `extract_stencil_n`) so the runtime can patch the fast and slow targets independently.
//!
//! `Ctx` is layout-compatible with the `IntOp` stencils on its first two fields (`prog`, `sp`),
//! so a guarded chain freely mixes guard + push/add/mul stencils over one context.

#![cfg_attr(tailcall, feature(explicit_tail_calls))]
#![allow(clippy::missing_safety_doc)]
#![allow(incomplete_features)]

/// A resolved variable slot. `is_int != 0` means the fast path may unbox `value`; otherwise the
/// guard deopts. (A stand-in for reading facet-value's tag inline — the guard MECHANISM is
/// real; a production guard would test the `Value` tag itself here.)
#[repr(C)]
pub struct VarSlot {
    pub is_int: i64,
    pub value: i64,
}

#[repr(C)]
pub struct Ctx {
    pub prog: *const u64,
    pub sp: *mut i64,
    pub vars: *const VarSlot,
    pub deopt: *mut u64,
}

extern "C" {
    /// Fast continuation: the speculation held.
    fn weavy_cont(cx: *mut Ctx);
    /// Deopt continuation: the type guard failed.
    fn weavy_deopt(cx: *mut Ctx);
}

/// Speculate that variable `prog[0]` is an integer.
#[no_mangle]
pub unsafe extern "C" fn weavy_guard_i64(cx: *mut Ctx) {
    let c = &mut *cx;
    let idx = *c.prog as usize;
    c.prog = c.prog.add(1);
    let slot = &*c.vars.add(idx);
    if slot.is_int != 0 {
        // Bet held: unbox onto the i64 stack and stay on the fast chain.
        *c.sp = slot.value;
        c.sp = c.sp.add(1);
        #[cfg(tailcall)]
        {
            become weavy_cont(cx);
        }
        #[cfg(not(tailcall))]
        {
            weavy_cont(cx);
        }
    } else {
        // Bet missed: flag the deopt and branch to the slow exit.
        *c.deopt = 1;
        #[cfg(tailcall)]
        {
            become weavy_deopt(cx);
        }
        #[cfg(not(tailcall))]
        {
            weavy_deopt(cx);
        }
    }
}
