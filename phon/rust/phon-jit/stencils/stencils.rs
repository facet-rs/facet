//! phon-jit stencils, in Rust. `build.rs` compiles this to an object with rustc
//! (the same LLVM that builds the rest of phon) and extracts each stencil's
//! machine code — and its `phon_cont` relocations — by symbol.
//!
//! The decode stencils thread state through a `*mut Ctx` and reach the next op by
//! tail-calling the external `phon_cont`; that branch's `BRANCH26` relocation is
//! the hole we patch at compile time to chain copies. Per-op immediates ride in
//! `Ctx.prog`.
//!
//! With `--cfg tailcall` (nightly), the continuation is a guaranteed tail call
//! (`become`): the whole chain runs in one stack frame as a series of jumps, no
//! per-op call/return. Without it (stable), it falls back to an ordinary call.
//!
//! ## Control flow: sequences
//!
//! The owned-sequence stencil ([`phon_stencil_sequence`]) is the first stencil
//! with a loop. It does its work in Rust — read the count, allocate the element
//! buffer, fill it, hand it to `from_raw_parts` — and reaches the element body as
//! a *call-program*: a separately compiled stencil chain that ends in a `ret`,
//! invoked once per element through a function pointer in the [`SeqInfo`]. The
//! buffer is allocated with the global Rust allocator (via thunks in [`Ctx`]), so
//! the `Vec` adopting it via `from_raw_parts` can free it. Every call here is
//! indirect (through a struct field) or the patched `phon_cont`, so the only
//! relocation a copied stencil carries is still `BRANCH26` to `phon_cont`.

#![cfg_attr(tailcall, feature(explicit_tail_calls))]
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
    /// `[offset, size, align]` triples (scalars) and `*const SeqInfo` slots
    /// (sequences), consumed in order.
    pub prog: *const u64,
    /// 0 = ok, 1 = ran out of input / malformed (e.g. length too large).
    pub status: u64,
    /// Allocate `size` bytes aligned to `align` with the global Rust allocator.
    /// Returns null on `size == 0` (the caller substitutes a dangling pointer).
    pub alloc: unsafe extern "C" fn(size: usize, align: usize) -> *mut u8,
    /// Free a buffer previously returned by `alloc` (same size/align).
    pub dealloc: unsafe extern "C" fn(ptr: *mut u8, size: usize, align: usize),
}

/// A sequence op's immediates, reached through a `*const SeqInfo` slot in
/// `Ctx.prog`. The element body is the chain entered at `element_entry`, driven
/// by the triples at `element_prog`.
#[repr(C)]
pub struct SeqInfo {
    /// Where the sequence handle lives, relative to `base`.
    pub field_offset: usize,
    /// Bytes between consecutive elements in the buffer (element size).
    pub stride: usize,
    /// Alignment of the element type.
    pub elem_align: usize,
    /// Minimum wire bytes one element occupies (length-vs-remaining check).
    pub min_wire: usize,
    /// Opaque per-type context passed to the thunks.
    pub thunks_ctx: *const (),
    /// `*list = Vec::from_raw_parts(ptr, len, cap)`.
    pub from_raw_parts:
        unsafe extern "C" fn(ctx: *const (), list: *mut u8, ptr: *mut u8, len: usize, cap: usize),
    /// Entry to the element body chain (a `*mut Ctx` function ending in `ret`).
    pub element_entry: unsafe extern "C" fn(cx: *mut Ctx),
    /// The element body's immediate triples (reset into `Ctx.prog` per element).
    pub element_prog: *const u64,
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

    // Word-wise copy of `size` bytes (size is a fused run length, any value).
    let dst = c.base.add(off);
    let mut i = 0;
    while size - i >= 8 {
        core::ptr::copy_nonoverlapping(src.add(i), dst.add(i), 8);
        i += 8;
    }
    if size - i >= 4 {
        core::ptr::copy_nonoverlapping(src.add(i), dst.add(i), 4);
        i += 4;
    }
    if size - i >= 2 {
        core::ptr::copy_nonoverlapping(src.add(i), dst.add(i), 2);
        i += 2;
    }
    if size - i >= 1 {
        core::ptr::copy_nonoverlapping(src.add(i), dst.add(i), 1);
    }

    c.wire = src.add(size);
    #[cfg(tailcall)]
    become phon_cont(cx);
    #[cfg(not(tailcall))]
    phon_cont(cx);
}

/// Decode an owned sequence into `base + field_offset`, then continue.
///
/// Reads a `u32` count (bounds-checked like `read_len`), allocates a
/// `count * stride` buffer aligned to `elem_align` with the global allocator
/// (count 0 → dangling pointer, cap 0, no allocation), runs the element body at
/// each slot, then adopts the buffer into the handle via `from_raw_parts`. On a
/// mid-fill element error the buffer is freed and the chain stops with
/// `status = 1`.
#[no_mangle]
pub unsafe extern "C" fn phon_stencil_sequence(cx: *mut Ctx) {
    let c = &mut *cx;
    let info = &*(*c.prog as *const SeqInfo);
    let outer_prog = c.prog.add(1);
    let outer_base = c.base;

    // Read the u32 count with a bounds check (no alignment padding, like
    // `read_len` -> `read_u32`).
    if (c.wire as usize).wrapping_add(4) > c.wire_end as usize {
        c.status = 1;
        return;
    }
    let mut count_bytes = [0u8; 4];
    core::ptr::copy_nonoverlapping(c.wire, count_bytes.as_mut_ptr(), 4);
    let count = u32::from_le_bytes(count_bytes) as usize;
    c.wire = c.wire.add(4);

    // Length-vs-remaining check: each element costs at least `min_wire` bytes.
    let remaining = (c.wire_end as usize) - (c.wire as usize);
    let min = if info.min_wire == 0 { 1 } else { info.min_wire };
    if count > remaining / min {
        c.status = 1;
        return;
    }

    // Allocate the element buffer (engine-owned). count 0 -> dangling aligned.
    let (buffer, cap, alloc_size) = if count == 0 {
        (info.elem_align as *mut u8, 0usize, 0usize)
    } else {
        let size = count * info.stride;
        let buf = (c.alloc)(size, info.elem_align);
        // alloc returns null only on size 0; size > 0 here, so a null means OOM.
        if buf.is_null() {
            c.status = 1;
            return;
        }
        (buf, count, size)
    };

    // Fill the buffer: run the element body at each slot. The body shares the
    // wire cursor through `Ctx`; reset `prog`/`base` per element.
    let mut i = 0;
    while i < count {
        c.prog = info.element_prog;
        c.base = buffer.add(i * info.stride);
        (info.element_entry)(cx);
        if c.status != 0 {
            // Free the buffer on a mid-fill failure (elements trivially droppable).
            if cap != 0 {
                (c.dealloc)(buffer, alloc_size, info.elem_align);
            }
            return;
        }
        i += 1;
    }

    // Adopt the buffer into the handle, restore the outer cursors, continue.
    c.base = outer_base;
    let list = c.base.add(info.field_offset);
    (info.from_raw_parts)(info.thunks_ctx, list, buffer, count, cap);
    c.prog = outer_prog;

    #[cfg(tailcall)]
    become phon_cont(cx);
    #[cfg(not(tailcall))]
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
