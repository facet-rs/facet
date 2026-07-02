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

/// A resolved variable slot: `tag` is the runtime type (0 = i64, 1 = f64, other = deopt) and
/// `bits` the unboxed payload (i64 value, or f64 bits). A guard checks `tag`; on a match it
/// pushes `bits` and stays fast, else it deopts. (Stand-in for reading facet-value's tag inline
/// — the guard MECHANISM is real; a production guard would test the `Value` tag itself here.)
#[repr(C)]
pub struct VarSlot {
    pub tag: i64,
    pub bits: i64,
}

/// Runtime type tags.
pub const TAG_I64: i64 = 0;
pub const TAG_F64: i64 = 1;

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

/// Speculate that variable `prog[0]` has runtime type `tag`: if so push its bits and stay on the
/// fast chain, else flag a deopt and branch to the slow exit.
macro_rules! guard {
    ($name:ident, $tag:expr) => {
        #[no_mangle]
        pub unsafe extern "C" fn $name(cx: *mut Ctx) {
            let c = &mut *cx;
            let idx = *c.prog as usize;
            c.prog = c.prog.add(1);
            let slot = &*c.vars.add(idx);
            if slot.tag == $tag {
                *c.sp = slot.bits;
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
    };
}

guard!(weavy_guard_i64, TAG_I64);
guard!(weavy_guard_f64, TAG_F64);
