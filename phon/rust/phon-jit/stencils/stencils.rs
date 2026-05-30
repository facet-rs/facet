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
    /// 0 = ok, 1 = ran out of input / malformed (e.g. length too large),
    /// 2 = content validation failed (e.g. non-UTF-8 `String`), 3 = bad `Option`
    /// presence byte (the byte is in `aux`), 4 = unmatched enum wire index (the
    /// index is in `aux`).
    pub status: u64,
    /// Auxiliary value carried alongside a rejection `status`: the bad presence
    /// byte (`status == 3`) or the unmatched enum wire index (`status == 4`).
    pub aux: u64,
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

/// A bulk byte-run op's immediates, reached through a `*const BytesInfo` slot in
/// `Ctx.prog`. Unlike [`SeqInfo`] there is no element body: the run is one bulk
/// word-wise copy, no per-element call-program. Mirrors `BytesOp` (non-UTF-8).
#[repr(C)]
pub struct BytesInfo {
    /// Where the owned handle (the `Vec`) lives, relative to `base`.
    pub field_offset: usize,
    /// Bytes per element (the element type's size).
    pub stride: usize,
    /// Alignment of the contiguous element buffer.
    pub elem_align: usize,
    /// Opaque per-type context passed to the thunks.
    pub thunks_ctx: *const (),
    /// `*list = Vec::from_raw_parts(ptr, len, cap)` — `cap == len` (element count).
    pub from_raw_parts:
        unsafe extern "C" fn(ctx: *const (), list: *mut u8, ptr: *mut u8, len: usize, cap: usize),
    /// Validate the run before adopting it (UTF-8 for `String`, a no-op for `Vec`).
    /// Reached as an *indirect* call, so it adds no relocation. Returns `true` if
    /// the bytes are valid; on `false` the stencil reports `status = 2`.
    pub validate: unsafe extern "C" fn(ptr: *const u8, len: usize) -> bool,
}

/// An optional op's immediates, reached through a `*const OptInfo` slot in
/// `Ctx.prog`. The some-body is the chain entered at `some_entry`, driven by the
/// triples at `some_prog`, and run with `base = scratch` (the engine-allocated
/// inner buffer) and the shared wire cursor. Mirrors `OptionOp`.
#[repr(C)]
pub struct OptInfo {
    /// Where the `Option<T>` handle lives, relative to `base`.
    pub field_offset: usize,
    /// The inner `T`'s size — the decode scratch buffer's size (0 → dangling).
    pub inner_size: usize,
    /// The inner `T`'s alignment — the decode scratch buffer's alignment.
    pub inner_align: usize,
    /// Opaque per-type context passed to the thunks.
    pub thunks_ctx: *const (),
    /// Initialize the option at `option` to `None`.
    pub init_none: unsafe extern "C" fn(ctx: *const (), option: *mut u8),
    /// Initialize the option at `option` to `Some(*value)`, moving the inner out
    /// of `value` (the engine then frees `value`'s storage without dropping).
    pub init_some: unsafe extern "C" fn(ctx: *const (), option: *mut u8, value: *mut u8),
    /// Entry to the some-body chain (a `*mut Ctx` function ending in `ret`).
    pub some_entry: unsafe extern "C" fn(cx: *mut Ctx),
    /// The some-body's immediate stream (reset into `Ctx.prog` for the inner).
    pub some_prog: *const u64,
}

/// One enum variant's decode immediates, pointed at (as an array) by
/// [`EnumInfo`]. The payload is the chain entered at `payload_entry`, driven by
/// the triples at `payload_prog`, run with the SAME outer base + shared wire.
#[repr(C)]
pub struct EnumVariantInfo {
    /// The `u32` wire index identifying this variant.
    pub wire_index: u32,
    /// The in-memory discriminant value (its low `tag_width` bytes) to write.
    pub selector: u64,
    /// Entry to the payload chain (a `*mut Ctx` function ending in `ret`).
    pub payload_entry: unsafe extern "C" fn(cx: *mut Ctx),
    /// The payload's immediate stream (reset into `Ctx.prog`).
    pub payload_prog: *const u64,
}

/// An enum op's immediates, reached through a `*const EnumInfo` slot in
/// `Ctx.prog`. Reads a `u32` wire index, finds the matching variant (an ordinary
/// loop over `variants[..variant_count]`, branches within the stencil — no
/// relocation), writes its in-memory discriminant, then runs its payload chain.
/// Mirrors `EnumOp`.
#[repr(C)]
pub struct EnumInfo {
    /// Where the in-memory discriminant lives, relative to `base`.
    pub tag_offset: usize,
    /// The discriminant's width in bytes (1/2/4/8).
    pub tag_width: usize,
    /// Pointer to the first of `variant_count` `EnumVariantInfo`.
    pub variants: *const EnumVariantInfo,
    /// Number of variants.
    pub variant_count: usize,
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

/// Decode a bulk byte run (non-UTF-8) into `base + field_offset`, then continue.
///
/// Reads a `u32` count (bounds-checked against `remaining / stride.max(1)`, like
/// `read_len`), pads the wire to `elem_align`, then bulk-copies `count * stride`
/// contiguous bytes from the wire into a freshly allocated, `elem_align`-aligned
/// buffer (count 0 → dangling pointer, cap 0, no allocation), and adopts it into
/// the handle via `from_raw_parts` with `cap == count` (the ELEMENT count). No
/// per-element loop: the run is one word-wise inline copy, so the only relocation
/// the copied stencil carries is the `phon_cont` `BRANCH26`.
#[no_mangle]
pub unsafe extern "C" fn phon_stencil_bytes(cx: *mut Ctx) {
    let c = &mut *cx;
    let info = &*(*c.prog as *const BytesInfo);
    let outer_prog = c.prog.add(1);

    // Read the u32 count with a bounds check (no alignment padding yet, like
    // `read_len` -> `read_u32`).
    if (c.wire as usize).wrapping_add(4) > c.wire_end as usize {
        c.status = 1;
        return;
    }
    let mut count_bytes = [0u8; 4];
    core::ptr::copy_nonoverlapping(c.wire, count_bytes.as_mut_ptr(), 4);
    let count = u32::from_le_bytes(count_bytes) as usize;
    c.wire = c.wire.add(4);

    // Length-vs-remaining check: each element costs at least `stride.max(1)`
    // bytes (mirrors `read_len(stride.max(1))`), measured before padding.
    let remaining = (c.wire_end as usize) - (c.wire as usize);
    let min = if info.stride == 0 { 1 } else { info.stride };
    if count > remaining / min {
        c.status = 1;
        return;
    }

    // Pad the wire to `elem_align`, measured from the message start.
    let pos = (c.wire as usize) - (c.wire_start as usize);
    let pad = info.elem_align.wrapping_sub(pos & (info.elem_align - 1)) & (info.elem_align - 1);
    let src = c.wire.add(pad);

    // The whole run must fit (the real bounds check for the bulk copy).
    let total = count * info.stride;
    if (src as usize).wrapping_add(total) > c.wire_end as usize {
        c.status = 1;
        return;
    }

    // Validate the run before allocating or adopting it (UTF-8 for `String`, a
    // no-op for `Vec`). Indirect call through `info.validate` — no relocation.
    // status 2 marks invalid content, distinct from the EOF/bounds status 1.
    if !(info.validate)(src, total) {
        c.status = 2;
        return;
    }

    // Allocate the element buffer (engine-owned). total 0 -> dangling aligned.
    let (buffer, cap) = if total == 0 {
        (info.elem_align as *mut u8, 0usize)
    } else {
        let buf = (c.alloc)(total, info.elem_align);
        // alloc returns null only on size 0; total > 0 here, so null means OOM.
        if buf.is_null() {
            c.status = 1;
            return;
        }
        // Word-wise copy of `total` bytes (a runtime length, any value). An inline
        // copy — never a `memcpy` libcall, which a copied stencil can't relocate.
        let mut i = 0;
        while total - i >= 8 {
            core::ptr::copy_nonoverlapping(src.add(i), buf.add(i), 8);
            i += 8;
        }
        if total - i >= 4 {
            core::ptr::copy_nonoverlapping(src.add(i), buf.add(i), 4);
            i += 4;
        }
        if total - i >= 2 {
            core::ptr::copy_nonoverlapping(src.add(i), buf.add(i), 2);
            i += 2;
        }
        if total - i >= 1 {
            core::ptr::copy_nonoverlapping(src.add(i), buf.add(i), 1);
        }
        // cap is the ELEMENT count, not the byte total.
        (buf, count)
    };

    c.wire = src.add(total);

    // Adopt the buffer into the handle, then continue.
    let list = c.base.add(info.field_offset);
    (info.from_raw_parts)(info.thunks_ctx, list, buffer, count, cap);
    c.prog = outer_prog;

    #[cfg(tailcall)]
    become phon_cont(cx);
    #[cfg(not(tailcall))]
    phon_cont(cx);
}

/// Decode an `Option<T>` into `base + field_offset`, then continue.
///
/// Reads a `u8` presence byte (bounds-checked). `0` → `init_none`. `1` → allocate
/// an `inner_size`/`inner_align` scratch buffer (size 0 → dangling, no alloc), run
/// the some-body chain at `base = scratch` sharing the wire cursor, then move the
/// inner into the option via `init_some` and free the scratch WITHOUT dropping
/// (ownership transferred). Any other presence byte rejects with `status = 3`
/// (the byte in `aux`). The presence branch is an ordinary `match` — rustc lowers
/// it to in-stencil branches, so the only relocation is the `phon_cont` BRANCH26.
#[no_mangle]
pub unsafe extern "C" fn phon_stencil_option(cx: *mut Ctx) {
    let c = &mut *cx;
    let info = &*(*c.prog as *const OptInfo);
    let outer_prog = c.prog.add(1);
    let outer_base = c.base;

    // Read the u8 presence byte with a bounds check (like `read_u8`).
    if (c.wire as usize).wrapping_add(1) > c.wire_end as usize {
        c.status = 1;
        return;
    }
    let presence = *c.wire;
    c.wire = c.wire.add(1);

    let option = outer_base.add(info.field_offset);
    match presence {
        0 => {
            (info.init_none)(info.thunks_ctx, option);
        }
        1 => {
            // Allocate scratch for the inner value. size 0 -> dangling aligned.
            let (scratch, alloc_size) = if info.inner_size == 0 {
                (info.inner_align as *mut u8, 0usize)
            } else {
                let buf = (c.alloc)(info.inner_size, info.inner_align);
                if buf.is_null() {
                    c.status = 1;
                    return;
                }
                (buf, info.inner_size)
            };
            // Run the some-body at `base = scratch`, sharing the wire cursor.
            c.prog = info.some_prog;
            c.base = scratch;
            (info.some_entry)(cx);
            if c.status != 0 {
                if alloc_size != 0 {
                    (c.dealloc)(scratch, alloc_size, info.inner_align);
                }
                return;
            }
            // Move the inner into the option, then free the scratch without
            // dropping (ownership transferred to the option).
            c.base = outer_base;
            (info.init_some)(info.thunks_ctx, option, scratch);
            if alloc_size != 0 {
                (c.dealloc)(scratch, alloc_size, info.inner_align);
            }
        }
        b => {
            // A presence byte other than 0/1 is hostile input: reject, carrying
            // the byte for a precise `InvalidBool` mapping in `run()`.
            c.status = 3;
            c.aux = b as u64;
            return;
        }
    }

    c.base = outer_base;
    c.prog = outer_prog;

    #[cfg(tailcall)]
    become phon_cont(cx);
    #[cfg(not(tailcall))]
    phon_cont(cx);
}

/// Decode a `#[repr(int)]` enum into the value at `base`, then continue.
///
/// Reads a `u32` wire index (bounds-checked), finds the variant whose `wire_index`
/// matches by an ordinary loop over `variants[..variant_count]` (in-stencil
/// branches, no relocation). No match → reject with `status = 4` (the index in
/// `aux`). On a match, writes the in-memory discriminant (`selector`'s low
/// `tag_width` bytes at `base + tag_offset`, like `write_uint`), then runs the
/// variant's payload chain at the SAME outer base sharing the wire cursor.
#[no_mangle]
pub unsafe extern "C" fn phon_stencil_enum(cx: *mut Ctx) {
    let c = &mut *cx;
    let info = &*(*c.prog as *const EnumInfo);
    let outer_prog = c.prog.add(1);
    let outer_base = c.base;

    // Read the u32 wire index with a bounds check (like `read_u32`).
    if (c.wire as usize).wrapping_add(4) > c.wire_end as usize {
        c.status = 1;
        return;
    }
    let mut idx_bytes = [0u8; 4];
    core::ptr::copy_nonoverlapping(c.wire, idx_bytes.as_mut_ptr(), 4);
    let wire_index = u32::from_le_bytes(idx_bytes);
    c.wire = c.wire.add(4);

    // Linear search for the matching variant (a plain loop — branches within the
    // stencil, no relocation).
    let mut found: *const EnumVariantInfo = core::ptr::null();
    let mut i = 0;
    while i < info.variant_count {
        let v = info.variants.add(i);
        if (*v).wire_index == wire_index {
            found = v;
            break;
        }
        i += 1;
    }
    if found.is_null() {
        // Unmatched index is hostile input: reject, carrying the index for a
        // precise `BadVariantIndex` mapping in `run()`.
        c.status = 4;
        c.aux = wire_index as u64;
        return;
    }
    let variant = &*found;

    // Write the in-memory discriminant (low `tag_width` bytes of `selector`,
    // little-endian). A `write_volatile` byte loop shifting bytes out of the
    // `u64` keeps LLVM from lowering this runtime-length copy to a `memcpy`
    // libcall (and avoids array indexing that would emit a `panic_bounds_check`
    // call) — neither relocation a copied stencil can carry; `tag_width <= 8` is
    // tiny.
    let disc = outer_base.add(info.tag_offset);
    let mut w = 0;
    while w < info.tag_width {
        core::ptr::write_volatile(disc.add(w), (variant.selector >> (w * 8)) as u8);
        w += 1;
    }

    // Run the payload chain at the SAME outer base, sharing the wire cursor.
    c.prog = variant.payload_prog;
    c.base = outer_base;
    (variant.payload_entry)(cx);
    if c.status != 0 {
        return;
    }

    c.base = outer_base;
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

// ============================================================================
// Encode stencils
// ============================================================================
//
// The mirror of decode: instead of reading a fixed wire slice into memory, the
// encode stencils read a value's in-memory bytes and append them to a *growing*
// output buffer. That growth is the only real difference from decode — a scalar
// might need more room than is left, so each stencil ensures capacity by calling
// the `grow` thunk (indirect through `EncCtx`, so it adds no relocation), then
// re-reads `out_ptr`/`out_cap`. The engine owns the `Vec<u8>`; `out_handle`
// points at it so `grow` can reserve through it (keeping its allocator), and the
// driver sets the `Vec`'s length to `out_pos` after the run.
//
// Like decode, the only relocation a copied encode stencil carries is the
// `BRANCH26` to its continuation (`phon_econt`); every other call is indirect
// through an `EncCtx` or `EncSeqInfo` field.

/// Encode-side threaded state, mirroring `Ctx`. Matched byte for byte by the Rust
/// driver in `src/native.rs`.
#[repr(C)]
pub struct EncCtx {
    /// Base pointer of the value being read.
    pub base: *const u8,
    /// `&mut Vec<u8>` the engine owns; `grow` reserves through it.
    pub out_handle: *mut u8,
    /// The output buffer's data pointer (re-read after every `grow`).
    pub out_ptr: *mut u8,
    /// Number of bytes written so far (the live length).
    pub out_pos: usize,
    /// The output buffer's current capacity.
    pub out_cap: usize,
    /// `[offset, size, align]` triples (scalars) and `*const EncSeqInfo` slots
    /// (sequences), consumed in order.
    pub prog: *const u64,
    /// Reserve so the buffer holds at least `needed` bytes, then write the new
    /// `out_ptr`/`out_cap` back into the `EncCtx`. The current live length is
    /// `out_pos`; bytes below it are preserved.
    pub grow: unsafe extern "C" fn(cx: *mut EncCtx, needed: usize),
}

/// An encode sequence op's immediates, reached through a `*const EncSeqInfo` slot
/// in `EncCtx.prog`. The element body is the chain entered at `element_entry`,
/// driven by the triples at `element_prog`. Mirrors `SeqInfo`, minus the
/// decode-only `from_raw_parts` (and plus the two read thunks).
#[repr(C)]
pub struct EncSeqInfo {
    /// Where the sequence handle lives, relative to `base`.
    pub field_offset: usize,
    /// Bytes between consecutive elements in the buffer (element size).
    pub stride: usize,
    /// Opaque per-type context passed to the thunks.
    pub thunks_ctx: *const (),
    /// The sequence's current element count.
    pub len: unsafe extern "C" fn(ctx: *const (), list: *const u8) -> usize,
    /// A pointer to the sequence's contiguous element storage (for reading).
    pub data: unsafe extern "C" fn(ctx: *const (), list: *const u8) -> *const u8,
    /// Entry to the element body chain (an `*mut EncCtx` function ending in `ret`).
    pub element_entry: unsafe extern "C" fn(cx: *mut EncCtx),
    /// The element body's immediate triples (reset into `EncCtx.prog` per element).
    pub element_prog: *const u64,
}

/// An encode bulk byte-run op's immediates, reached through a `*const EncBytesInfo`
/// slot in `EncCtx.prog`. Mirrors `EncSeqInfo` minus the element body: the run is
/// one bulk word-wise copy, no per-element loop.
#[repr(C)]
pub struct EncBytesInfo {
    /// Where the owned handle (the `Vec`) lives, relative to `base`.
    pub field_offset: usize,
    /// Bytes per element (the element type's size).
    pub stride: usize,
    /// Alignment of the contiguous element buffer (wire padding before the run).
    pub elem_align: usize,
    /// Opaque per-type context passed to the thunks.
    pub thunks_ctx: *const (),
    /// The handle's current element count.
    pub len: unsafe extern "C" fn(ctx: *const (), list: *const u8) -> usize,
    /// A pointer to the handle's contiguous element storage (for reading).
    pub data: unsafe extern "C" fn(ctx: *const (), list: *const u8) -> *const u8,
}

/// An encode optional op's immediates, reached through a `*const EncOptInfo` slot
/// in `EncCtx.prog`. Mirrors `OptInfo` minus the decode-only init thunks (plus the
/// read thunks). The some-body is run with `base = get_value(...)`.
#[repr(C)]
pub struct EncOptInfo {
    /// Where the `Option<T>` handle lives, relative to `base`.
    pub field_offset: usize,
    /// Opaque per-type context passed to the thunks.
    pub thunks_ctx: *const (),
    /// Whether the option at `option` is `Some`.
    pub is_some: unsafe extern "C" fn(ctx: *const (), option: *const u8) -> bool,
    /// A pointer to the contained value (valid only when `is_some`).
    pub get_value: unsafe extern "C" fn(ctx: *const (), option: *const u8) -> *const u8,
    /// Entry to the some-body chain (a `*mut EncCtx` function ending in `ret`).
    pub some_entry: unsafe extern "C" fn(cx: *mut EncCtx),
    /// The some-body's immediate stream (reset into `EncCtx.prog` for the inner).
    pub some_prog: *const u64,
}

/// One enum variant's encode immediates, pointed at (as an array) by
/// [`EncEnumInfo`]. The payload chain is run with the SAME outer base.
#[repr(C)]
pub struct EncEnumVariantInfo {
    /// The `u32` wire index to write for this variant.
    pub wire_index: u32,
    /// The in-memory discriminant value (its low `tag_width` bytes) identifying
    /// this variant — matched against the read discriminant, masked to `tag_width`.
    pub selector: u64,
    /// Entry to the payload chain (a `*mut EncCtx` function ending in `ret`).
    pub payload_entry: unsafe extern "C" fn(cx: *mut EncCtx),
    /// The payload's immediate stream (reset into `EncCtx.prog`).
    pub payload_prog: *const u64,
}

/// An encode enum op's immediates, reached through a `*const EncEnumInfo` slot in
/// `EncCtx.prog`. Reads the in-memory discriminant (`tag_width` bytes at
/// `base + tag_offset`, like `read_uint`), finds the variant whose `selector`
/// matches (masked to `tag_width`) by a plain loop, writes its `u32` wire index,
/// then runs its payload chain. Mirrors `EnumOp`.
#[repr(C)]
pub struct EncEnumInfo {
    /// Where the in-memory discriminant lives, relative to `base`.
    pub tag_offset: usize,
    /// The discriminant's width in bytes (1/2/4/8).
    pub tag_width: usize,
    /// Pointer to the first of `variant_count` `EncEnumVariantInfo`.
    pub variants: *const EncEnumVariantInfo,
    /// Number of variants.
    pub variant_count: usize,
}

extern "C" {
    fn phon_econt(cx: *mut EncCtx);
}

/// Encode one fixed-width scalar from `base + offset`, then continue.
///
/// Pads the output to `align` with zero bytes (measured from the buffer start),
/// ensures capacity for the pad plus `size` bytes (growing if needed), copies the
/// scalar's bytes out, and advances `out_pos`.
#[no_mangle]
pub unsafe extern "C" fn phon_stencil_scalar_enc(cx: *mut EncCtx) {
    let c = &mut *cx;
    let off = *c.prog as usize;
    let size = *c.prog.add(1) as usize;
    let align = *c.prog.add(2) as usize;
    c.prog = c.prog.add(3);

    // Pad to alignment, measured from the output start (the live length).
    let pad = align.wrapping_sub(c.out_pos & (align - 1)) & (align - 1);
    let need = c.out_pos + pad + size;

    // Ensure capacity for the padding and the scalar; re-read ptr/cap after.
    if need > c.out_cap {
        (c.grow)(cx, need);
    }

    // Zero the padding bytes, then copy the scalar out. `write_volatile` keeps
    // LLVM from lowering the loop to a `bzero`/`memset` libcall — that would add
    // an external relocation a copied stencil can't carry; we only ever patch the
    // `BRANCH26` to the continuation. `pad < align` is small, so the byte loop is
    // cheap.
    let mut dst = c.out_ptr.add(c.out_pos);
    let mut k = 0;
    while k < pad {
        core::ptr::write_volatile(dst, 0u8);
        dst = dst.add(1);
        k += 1;
    }

    let src = c.base.add(off);
    // Word-wise copy of `size` bytes (size is a fused run length, any value).
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

    c.out_pos += pad + size;
    #[cfg(tailcall)]
    become phon_econt(cx);
    #[cfg(not(tailcall))]
    phon_econt(cx);
}

/// Encode an owned sequence from `base + field_offset`, then continue.
///
/// Reads the element count via the `len` thunk, writes it as a `u32` (no
/// alignment, like `write_u32`), gets the element storage pointer via the `data`
/// thunk, and runs the element body once per element with `base = data +
/// i*stride`. The element body shares the output cursor through `EncCtx`.
#[no_mangle]
pub unsafe extern "C" fn phon_stencil_sequence_enc(cx: *mut EncCtx) {
    let c = &mut *cx;
    let info = &*(*c.prog as *const EncSeqInfo);
    let outer_prog = c.prog.add(1);
    let outer_base = c.base;

    let list = outer_base.add(info.field_offset);
    let count = (info.len)(info.thunks_ctx, list);

    // Write the u32 count (no alignment padding, like `write_u32`).
    let need = c.out_pos + 4;
    if need > c.out_cap {
        (c.grow)(cx, need);
    }
    let count_bytes = (count as u32).to_le_bytes();
    core::ptr::copy_nonoverlapping(count_bytes.as_ptr(), c.out_ptr.add(c.out_pos), 4);
    c.out_pos += 4;

    // Encode each element. The body shares the output cursor through `EncCtx`;
    // reset `prog`/`base` per element.
    let data = (info.data)(info.thunks_ctx, list);
    let mut i = 0;
    while i < count {
        c.prog = info.element_prog;
        c.base = data.add(i * info.stride);
        (info.element_entry)(cx);
        i += 1;
    }

    // Restore the outer cursors and continue.
    c.base = outer_base;
    c.prog = outer_prog;

    #[cfg(tailcall)]
    become phon_econt(cx);
    #[cfg(not(tailcall))]
    phon_econt(cx);
}

/// Encode a bulk byte run (non-UTF-8) from `base + field_offset`, then continue.
///
/// Reads the element count via the `len` thunk, writes it as a `u32` (no
/// alignment, like `write_u32`), pads the output to `elem_align`, gets the
/// contiguous element storage via the `data` thunk, then bulk-copies
/// `count * stride` bytes out in one inline word-wise run (no per-element loop, no
/// `memcpy` libcall). The only relocation the copied stencil carries is the
/// `phon_econt` `BRANCH26`.
#[no_mangle]
pub unsafe extern "C" fn phon_stencil_bytes_enc(cx: *mut EncCtx) {
    let c = &mut *cx;
    let info = &*(*c.prog as *const EncBytesInfo);
    let outer_prog = c.prog.add(1);

    let list = c.base.add(info.field_offset);
    let count = (info.len)(info.thunks_ctx, list);
    let total = count * info.stride;

    // Write the u32 count (no alignment padding, like `write_u32`), then pad the
    // output to `elem_align`, then ensure room for the whole run.
    let pad = info.elem_align.wrapping_sub((c.out_pos + 4) & (info.elem_align - 1))
        & (info.elem_align - 1);
    let need = c.out_pos + 4 + pad + total;
    if need > c.out_cap {
        (c.grow)(cx, need);
    }

    let count_bytes = (count as u32).to_le_bytes();
    core::ptr::copy_nonoverlapping(count_bytes.as_ptr(), c.out_ptr.add(c.out_pos), 4);
    c.out_pos += 4;

    // Zero the padding bytes. `write_volatile` keeps LLVM from lowering the loop
    // to a `bzero`/`memset` libcall a copied stencil can't relocate; `pad < align`
    // is small.
    let mut dst = c.out_ptr.add(c.out_pos);
    let mut k = 0;
    while k < pad {
        core::ptr::write_volatile(dst, 0u8);
        dst = dst.add(1);
        k += 1;
    }
    c.out_pos += pad;

    // Bulk-copy `total` bytes from the contiguous element storage. Word-wise
    // inline copy of a runtime length — never a `memcpy` libcall.
    let srcp = (info.data)(info.thunks_ctx, list);
    let dstp = c.out_ptr.add(c.out_pos);
    let mut i = 0;
    while total - i >= 8 {
        core::ptr::copy_nonoverlapping(srcp.add(i), dstp.add(i), 8);
        i += 8;
    }
    if total - i >= 4 {
        core::ptr::copy_nonoverlapping(srcp.add(i), dstp.add(i), 4);
        i += 4;
    }
    if total - i >= 2 {
        core::ptr::copy_nonoverlapping(srcp.add(i), dstp.add(i), 2);
        i += 2;
    }
    if total - i >= 1 {
        core::ptr::copy_nonoverlapping(srcp.add(i), dstp.add(i), 1);
    }
    c.out_pos += total;

    c.prog = outer_prog;

    #[cfg(tailcall)]
    become phon_econt(cx);
    #[cfg(not(tailcall))]
    phon_econt(cx);
}

/// Encode an `Option<T>` from `base + field_offset`, then continue.
///
/// `is_some`? then write a `u8` `1` and run the some-body chain at
/// `base = get_value(...)`, sharing the output cursor; else write a `u8` `0`. The
/// presence branch is an ordinary `if` — rustc lowers it to an in-stencil branch,
/// so the only relocation is the `phon_econt` BRANCH26.
#[no_mangle]
pub unsafe extern "C" fn phon_stencil_option_enc(cx: *mut EncCtx) {
    let c = &mut *cx;
    let info = &*(*c.prog as *const EncOptInfo);
    let outer_prog = c.prog.add(1);
    let outer_base = c.base;

    let option = outer_base.add(info.field_offset);
    let present = (info.is_some)(info.thunks_ctx, option);

    // Write the u8 presence byte (no alignment, like `write_u8`).
    let need = c.out_pos + 1;
    if need > c.out_cap {
        (c.grow)(cx, need);
    }
    *c.out_ptr.add(c.out_pos) = if present { 1 } else { 0 };
    c.out_pos += 1;

    if present {
        // Run the some-body at `base = get_value(...)`, sharing the output cursor.
        let inner = (info.get_value)(info.thunks_ctx, option);
        c.prog = info.some_prog;
        c.base = inner;
        (info.some_entry)(cx);
    }

    c.base = outer_base;
    c.prog = outer_prog;

    #[cfg(tailcall)]
    become phon_econt(cx);
    #[cfg(not(tailcall))]
    phon_econt(cx);
}

/// Encode a `#[repr(int)]` enum from the value at `base`, then continue.
///
/// Reads the in-memory discriminant (`tag_width` bytes at `base + tag_offset`,
/// like `read_uint`), finds the variant whose `selector` matches (masked to
/// `tag_width`) by an ordinary loop (in-stencil branches, no relocation), writes
/// its `u32` wire index, then runs its payload chain at the SAME outer base
/// sharing the output cursor. An unmatched discriminant cannot arise from a valid
/// in-memory enum, so (like the interpreter's `.expect`) the loop falls through to
/// writing nothing and continuing — but a well-formed value always matches.
#[no_mangle]
pub unsafe extern "C" fn phon_stencil_enum_enc(cx: *mut EncCtx) {
    let c = &mut *cx;
    let info = &*(*c.prog as *const EncEnumInfo);
    let outer_prog = c.prog.add(1);
    let outer_base = c.base;

    // Read the in-memory discriminant (low `tag_width` bytes, little-endian),
    // masked. A `read_volatile` byte loop accumulating with shifts keeps LLVM
    // from lowering this runtime-length copy to a `memcpy` libcall (and avoids
    // array indexing that would emit a `panic_bounds_check` call) — neither
    // relocation a copied stencil can carry; `tag_width <= 8` is tiny.
    let src = outer_base.add(info.tag_offset);
    let mut disc: u64 = 0;
    let mut r = 0;
    while r < info.tag_width {
        disc |= (core::ptr::read_volatile(src.add(r)) as u64) << (r * 8);
        r += 1;
    }
    let mask = if info.tag_width >= 8 { u64::MAX } else { (1u64 << (info.tag_width * 8)) - 1 };

    // Linear search for the matching variant (a plain loop — branches within the
    // stencil, no relocation).
    let mut found: *const EncEnumVariantInfo = core::ptr::null();
    let mut i = 0;
    while i < info.variant_count {
        let v = info.variants.add(i);
        if ((*v).selector & mask) == (disc & mask) {
            found = v;
            break;
        }
        i += 1;
    }
    if found.is_null() {
        // A valid in-memory enum always matches; nothing to write otherwise.
        c.base = outer_base;
        c.prog = outer_prog;
        #[cfg(tailcall)]
        become phon_econt(cx);
        #[cfg(not(tailcall))]
        return phon_econt(cx);
    }
    let variant = &*found;

    // Write the u32 wire index (no alignment, like `write_u32`).
    let need = c.out_pos + 4;
    if need > c.out_cap {
        (c.grow)(cx, need);
    }
    let idx_bytes = variant.wire_index.to_le_bytes();
    core::ptr::copy_nonoverlapping(idx_bytes.as_ptr(), c.out_ptr.add(c.out_pos), 4);
    c.out_pos += 4;

    // Run the payload chain at the SAME outer base, sharing the output cursor.
    c.prog = variant.payload_prog;
    c.base = outer_base;
    (variant.payload_entry)(cx);

    c.base = outer_base;
    c.prog = outer_prog;

    #[cfg(tailcall)]
    become phon_econt(cx);
    #[cfg(not(tailcall))]
    phon_econt(cx);
}

/// Terminal encode stencil: stop. Mirrors `phon_stencil_done`.
#[no_mangle]
pub unsafe extern "C" fn phon_stencil_done_enc(_cx: *mut EncCtx) {}
