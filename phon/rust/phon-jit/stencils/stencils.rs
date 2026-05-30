//! phon-jit stencils, in Rust. `build.rs` compiles this to an object with rustc
//! (the same LLVM that builds the rest of phon) and extracts each stencil's
//! machine code — and its `phon_cont` relocations — by symbol.
//!
//! The decode stencils thread state through a `*mut Ctx` and reach the next op by
//! calling the external `phon_cont`; that call's `BRANCH26` relocation is the
//! hole we patch at compile time to chain copies. Per-op immediates ride in
//! `Ctx.prog` for now (baking them into the code is the later step).

#![allow(clippy::missing_safety_doc)]

#[repr(C)]
pub struct Ctx {
    /// Current wire cursor.
    pub wire: *const u8,
    /// Start of the message (alignment is measured from here).
    pub wire_start: *const u8,
    /// One past the last wire byte.
    pub wire_end: *const u8,
    /// Base pointer of the value being built.
    pub base: *mut u8,
    /// `[offset, size, align]` triples, one per scalar op, consumed in order.
    pub prog: *const u64,
    /// 0 = ok, 1 = ran out of input.
    pub status: u64,
}

extern "C" {
    fn phon_cont(cx: *mut Ctx);
}

/// Decode one fixed-width scalar into `base + offset`, then continue.
#[no_mangle]
pub unsafe extern "C" fn phon_stencil_scalar(cx: *mut Ctx) {
    let c = &mut *cx;
    let off = *c.prog as usize;
    let size = *c.prog.add(1) as usize;
    let align = *c.prog.add(2) as usize;
    c.prog = c.prog.add(3);

    // Pad to alignment, measured from the message start.
    let pos = (c.wire as usize) - (c.wire_start as usize);
    let pad = align.wrapping_sub(pos & (align - 1)) & (align - 1);
    let src = c.wire.add(pad);

    // Hostile-input bounds check: the padded read must stay in the buffer.
    if (src as usize).wrapping_add(size) > c.wire_end as usize {
        c.status = 1;
        return;
    }

    let dst = c.base.add(off);
    match size {
        0 => {}
        1 => core::ptr::copy_nonoverlapping(src, dst, 1),
        2 => core::ptr::copy_nonoverlapping(src, dst, 2),
        4 => core::ptr::copy_nonoverlapping(src, dst, 4),
        8 => core::ptr::copy_nonoverlapping(src, dst, 8),
        16 => core::ptr::copy_nonoverlapping(src, dst, 16),
        n => {
            let mut i = 0;
            while i < n {
                *dst.add(i) = *src.add(i);
                i += 1;
            }
        }
    }

    c.wire = src.add(size);
    phon_cont(cx);
}

/// Terminal stencil: stop, leaving `status` unchanged (0 = ok).
#[no_mangle]
pub unsafe extern "C" fn phon_stencil_done(_cx: *mut Ctx) {}

/// Spine self-test stencil: `x * 3 + 1`. No relocations.
#[no_mangle]
pub extern "C" fn phon_stencil_smoke(x: i64) -> i64 {
    x.wrapping_mul(3).wrapping_add(1)
}
