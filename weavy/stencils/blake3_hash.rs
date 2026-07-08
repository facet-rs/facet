//! AArch64 NEON BLAKE3 stencil entrypoints for the hash-as-field spike.
//!
//! Frame words hold raw pointers/lengths. The entrypoints are intentionally
//! task-shaped copied-code calls rather than a public task op: this is the
//! microbench harness for testing whether the stencil form can express the hot
//! hash shapes without a host-call boundary.

#![cfg_attr(tailcall, feature(explicit_tail_calls))]
#![allow(incomplete_features)]

#[cfg(target_arch = "aarch64")]
mod blake3_neon_core;

#[repr(C)]
pub struct Ctx {
    pub prog: *const u64,
    pub frame: *mut u8,
    pub ready: *mut i64,
    pub awaited: *const i64,
    pub resume: *mut u64,
    pub await_index: *mut u64,
    pub exit: *mut i64,
}

extern "C" {
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

#[inline(always)]
unsafe fn read_u64(frame: *mut u8, off: u64) -> u64 {
    (frame.add(off as usize) as *const u64).read_unaligned()
}

/// Hash one <=1KiB value.
///
/// Immediates: [input_ptr_off, len_off, out_ptr_off].
#[cfg(target_arch = "aarch64")]
#[no_mangle]
pub unsafe extern "C" fn weavy_blake3_hash_small(cx: *mut Ctx) {
    let c = &mut *cx;
    let input_ptr_off = *c.prog;
    let len_off = *c.prog.add(1);
    let out_ptr_off = *c.prog.add(2);
    c.prog = c.prog.add(3);

    let input = read_u64(c.frame, input_ptr_off) as *const u8;
    let len = read_u64(c.frame, len_off) as usize;
    let out = read_u64(c.frame, out_ptr_off) as *mut u8;
    blake3_neon_core::hash_small_neon(input, len, out);
    cont!(cx);
}

/// Fold a carried chaining value and one new 32-byte element hash.
///
/// Immediates: [left_cv_ptr_off, right_cv_ptr_off, out_ptr_off].
#[cfg(target_arch = "aarch64")]
#[no_mangle]
pub unsafe extern "C" fn weavy_blake3_fold_parent(cx: *mut Ctx) {
    let c = &mut *cx;
    let left_ptr_off = *c.prog;
    let right_ptr_off = *c.prog.add(1);
    let out_ptr_off = *c.prog.add(2);
    c.prog = c.prog.add(3);

    let left = read_u64(c.frame, left_ptr_off) as *const u8;
    let right = read_u64(c.frame, right_ptr_off) as *const u8;
    let out = read_u64(c.frame, out_ptr_off) as *mut u8;
    blake3_neon_core::fold_parent_neon(left, right, out);
    cont!(cx);
}

/// Hash a cache-resident batch of equally sized values under one copied-code
/// entry.
///
/// Immediates: [input_ptr_off, len_off, count_off, stride_off, out_ptr_off].
#[cfg(target_arch = "aarch64")]
#[no_mangle]
pub unsafe extern "C" fn weavy_blake3_hash_batch(cx: *mut Ctx) {
    let c = &mut *cx;
    let input_ptr_off = *c.prog;
    let len_off = *c.prog.add(1);
    let count_off = *c.prog.add(2);
    let stride_off = *c.prog.add(3);
    let out_ptr_off = *c.prog.add(4);
    c.prog = c.prog.add(5);

    let input = read_u64(c.frame, input_ptr_off) as *const u8;
    let len = read_u64(c.frame, len_off) as usize;
    let count = read_u64(c.frame, count_off) as usize;
    let stride = read_u64(c.frame, stride_off) as usize;
    let out = read_u64(c.frame, out_ptr_off) as *mut u8;
    blake3_neon_core::hash_batch_neon(input, len, count, stride, out);
    cont!(cx);
}
