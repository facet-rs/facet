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
    /// index is in `aux`), 5 = writer-only enum variant (the index is in `aux`),
    /// 6 = duplicate map key, 7 = opaque adapter rejected input.
    pub status: u64,
    /// Auxiliary value carried alongside a rejection `status`: the bad presence
    /// byte (`status == 3`) or the unmatched enum wire index (`status == 4`).
    pub aux: u64,
    /// Optional caller-owned error slot. `status == 8` means a helper wrote the
    /// exact `DecodeError` here.
    pub error: *mut (),
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

/// A borrowed, zero-copy byte-run op's immediates, reached through a
/// `*const BorrowInfo` slot in `Ctx.prog`. Like [`BytesInfo`] there is no element
/// body, and like it the run is bounds-checked — but decode writes a fat pointer
/// straight INTO the input (no allocation, no copy) via the `set_borrowed`
/// construct thunk. Mirrors `BorrowOp`.
#[repr(C)]
pub struct BorrowInfo {
    /// Where the borrowed handle (the `&str`/`&[u8]` fat pointer) lives, relative
    /// to `base`.
    pub field_offset: usize,
    /// Bytes per element (1 for `&str`/`&[u8]`).
    pub stride: usize,
    /// Alignment of the borrowed run on the wire (1 for `&str`/`&[u8]`).
    pub elem_align: usize,
    /// Opaque per-type context passed to the thunk.
    pub thunks_ctx: *const (),
    /// Construct the borrowed value at `field`, pointing it at `ptr[..len]` (a run
    /// INTO the input). Reached as an *indirect* call, so it adds no relocation.
    /// Returns `true` if the content is valid; on `false` the stencil reports
    /// `status = 2` (e.g. non-UTF-8 `&str`).
    pub set_borrowed:
        unsafe extern "C" fn(ctx: *const (), field: *mut u8, ptr: *const u8, len: usize) -> bool,
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

/// A result op's immediates, reached through a `*const ResultInfo` slot in
/// `Ctx.prog`. Mirrors `ResultOp`: a `u32` wire index dispatches to the Ok or Err
/// sub-chain, each decoded into scratch and moved into the uninitialized
/// `Result<T, E>` through the bound result thunk.
#[repr(C)]
pub struct ResultInfo {
    /// Where the `Result<T, E>` handle lives, relative to `base`.
    pub field_offset: usize,
    /// The Ok payload's scratch size and alignment.
    pub ok_size: usize,
    pub ok_align: usize,
    /// The Err payload's scratch size and alignment.
    pub err_size: usize,
    pub err_align: usize,
    /// Wire indices for the Ok and Err arms (single-schema or compat writer indices).
    pub ok_wire_index: u32,
    pub err_wire_index: u32,
    /// Opaque per-type context passed to the thunks.
    pub thunks_ctx: *const (),
    /// Initialize the result at `result` to `Ok(*value)`.
    pub init_ok: unsafe extern "C" fn(ctx: *const (), result: *mut u8, value: *mut u8),
    /// Initialize the result at `result` to `Err(*value)`.
    pub init_err: unsafe extern "C" fn(ctx: *const (), result: *mut u8, value: *mut u8),
    /// Entry/prog for the Ok arm body.
    pub ok_entry: unsafe extern "C" fn(cx: *mut Ctx),
    pub ok_prog: *const u64,
    /// Entry/prog for the Err arm body.
    pub err_entry: unsafe extern "C" fn(cx: *mut Ctx),
    pub err_prog: *const u64,
}

/// An opaque op's immediates, reached through a `*const OpaqueInfo` slot in
/// `Ctx.prog`. Mirrors `OpaqueOp`: the stencil frames a length-prefixed byte span
/// and the adapter thunk builds the field value from a borrowed slice.
#[repr(C)]
pub struct OpaqueInfo {
    /// Where the opaque field lives, relative to `base`.
    pub field_offset: usize,
    /// Opaque per-field context passed to the thunk.
    pub thunks_ctx: *const (),
    /// Build the opaque value at `slot` from `bytes[..len]`.
    pub decode:
        unsafe extern "C" fn(ctx: *const (), bytes: *const u8, len: usize, slot: *mut u8) -> bool,
}

/// A dynamic `Value` op's immediates, reached through a `*const DynamicInfo` slot
/// in `Ctx.prog`. The helper owns the self-describing decoder and writes the exact
/// `DecodeError` through `Ctx.error` on failure.
#[repr(C)]
pub struct DynamicInfo {
    /// Where the `Value` lives, relative to `base`.
    pub field_offset: usize,
    /// Decode one self-describing value from `wire[..len]` into `slot`.
    pub read: unsafe extern "C" fn(
        wire: *const u8,
        len: usize,
        slot: *mut u8,
        consumed: *mut usize,
        error: *mut (),
    ) -> bool,
}

/// A recursive block call's immediates, reached through a `*const CallBlockInfo`
/// slot in `Ctx.prog`.
#[repr(C)]
pub struct CallBlockInfo {
    /// Where the recursive value lives, relative to the current base.
    pub offset: usize,
    /// Entry/prog for the precompiled recursive block.
    pub entry: unsafe extern "C" fn(cx: *mut Ctx),
    pub prog: *const u64,
}

/// An owned-map op's immediates, reached through a `*const MapInfo` slot in
/// `Ctx.prog`. A map is the most involved op: a LOOP with TWO sub-chains. The key
/// sub-chain is entered at `key_entry` driven by `key_prog`; the value sub-chain
/// at `value_entry` driven by `value_prog`. Each runs with `base = scratch` (the
/// engine-allocated key/value buffer) and the shared wire cursor. Mirrors `MapOp`:
/// per entry decode a key+value into scratch, `insert` (moving both in), then free
/// the scratch WITHOUT dropping.
#[repr(C)]
pub struct MapInfo {
    /// Where the map handle lives, relative to `base`.
    pub field_offset: usize,
    /// The key type's size — the decode key-scratch size (0 → dangling).
    pub key_size: usize,
    /// The key type's alignment — the decode key-scratch alignment.
    pub key_align: usize,
    /// The value type's size — the decode value-scratch size (0 → dangling).
    pub value_size: usize,
    /// The value type's alignment — the decode value-scratch alignment.
    pub value_align: usize,
    /// Opaque per-type context passed to the thunks.
    pub thunks_ctx: *const (),
    /// The map's current entry count (for the post-loop duplicate-key check).
    pub len: unsafe extern "C" fn(ctx: *const (), map: *const u8) -> usize,
    /// Initialize the uninitialized map at `map` with room for `cap` entries.
    pub init_with_capacity: unsafe extern "C" fn(ctx: *const (), map: *mut u8, cap: usize),
    /// Insert `(*key, *value)` into the initialized map, moving both out of their
    /// buffers (the engine then frees both without dropping).
    pub insert: unsafe extern "C" fn(ctx: *const (), map: *mut u8, key: *mut u8, value: *mut u8),
    /// Entry to the key sub-chain (a `*mut Ctx` function ending in `ret`).
    pub key_entry: unsafe extern "C" fn(cx: *mut Ctx),
    /// The key sub-chain's immediate stream (reset into `Ctx.prog`).
    pub key_prog: *const u64,
    /// Entry to the value sub-chain (a `*mut Ctx` function ending in `ret`).
    pub value_entry: unsafe extern "C" fn(cx: *mut Ctx),
    /// The value sub-chain's immediate stream (reset into `Ctx.prog`).
    pub value_prog: *const u64,
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
    /// Pointer to the first of `writer_only_count` `u32` wire indices the writer
    /// has but the reader removed (`r[compat.enum]`). A received index found here
    /// is a `WriterOnlyVariant` rejection (`status = 5`), distinct from a garbage
    /// index (`status = 4`).
    pub writer_only: *const u32,
    /// Number of writer-only variant indices.
    pub writer_only_count: usize,
}

/// A reader-only-default op's immediates, reached through a `*const DefaultInfo`
/// slot in `Ctx.prog`. Writes the reader field's default into `base + offset` with
/// NO wire interaction, by an *indirect* call to `thunk` (so it adds no
/// relocation). Mirrors `DefaultOp`; decode-only. (`r[compat.reader-only-fields]`.)
#[repr(C)]
pub struct DefaultInfo {
    /// Where the reader field lives, relative to `base`.
    pub offset: usize,
    /// Opaque per-field context the front door bound (passed to `thunk`).
    pub ctx: *const (),
    /// Initialize the uninitialized reader field at `slot` to its default.
    pub thunk: unsafe extern "C" fn(ctx: *const (), slot: *mut u8),
}

/// A writer-only-skip op's immediates, reached through a `*const SkipInfo` slot in
/// `Ctx.prog`. Advances the wire cursor past one writer value without touching the
/// reader's memory, by an *indirect* call to `walk` (so it adds no relocation).
/// Mirrors `SkipOp`; decode-only. (`r[compat.skip-writer-only]`.)
#[repr(C)]
pub struct SkipInfo {
    /// Opaque pointer to the `SkipOp` tree, passed back to `walk` untouched.
    pub skip_op: *const (),
    /// Advance `wire` over `[wire_start, wire_end)` per the `SkipOp` at
    /// `skip_op`. `wire_start` preserves compact alignment semantics for
    /// writer-only values skipped mid-message. Returns the new (advanced) cursor
    /// on success, or null on a skip failure (truncation, bad presence byte, or
    /// unmatched enum index).
    pub walk: unsafe extern "C" fn(
        skip_op: *const (),
        wire_start: *const u8,
        wire: *const u8,
        wire_end: *const u8,
    ) -> *const u8,
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
    // A zero-sized element (`min_wire == 0`, e.g. an empty struct) leaves the
    // count unbounded by the buffer, so a fixed cap applies — mirroring
    // `Reader::read_len`'s `ZST_COUNT_CAP` (1 << 24).
    let remaining = (c.wire_end as usize) - (c.wire as usize);
    let max = if info.min_wire == 0 { 1usize << 24 } else { remaining / info.min_wire };
    if count > max {
        c.status = 1;
        return;
    }

    // Allocate the element buffer (engine-owned). A zero total byte size — an
    // empty sequence OR any number of zero-sized elements (`stride == 0`) — must
    // not reach the allocator (a zero-size request is UB / returns null). Use a
    // dangling aligned pointer with cap == count, exactly as `Vec` does for ZSTs
    // (`size_of::<T>() * cap == 0` matches the empty allocation).
    let (buffer, cap, alloc_size) = if count == 0 || info.stride == 0 {
        (info.elem_align as *mut u8, count, 0usize)
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
            // Only a real, non-zero-size allocation needs freeing (a ZST run has a
            // dangling pointer and `alloc_size == 0`).
            if alloc_size != 0 {
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
// r[impl compact.alignment]
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

    // Pad before element bytes only when there is at least one element. An empty
    // run has no element storage, so the following field starts right after the
    // count, matching the interpreter.
    let pad = if count > 0 {
        let pos = (c.wire as usize) - (c.wire_start as usize);
        info.elem_align.wrapping_sub(pos & (info.elem_align - 1)) & (info.elem_align - 1)
    } else {
        0
    };
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

/// Decode a BORROWED, zero-copy byte run (`&str`/`&[u8]`) into `base +
/// field_offset`, then continue.
///
/// Reads a `u32` count (bounds-checked against `remaining / stride.max(1)`, like
/// `read_len`), pads the wire to `elem_align`, then bounds-checks the
/// `count * stride` run. Rather than allocate and copy (as `phon_stencil_bytes`
/// does), it calls `set_borrowed(ctx, base + field_offset, src, count)` where `src`
/// is the wire cursor pointing INTO the input — NO allocation, NO copy. The written
/// `&str`/`&[u8]` borrows the input buffer, which the caller keeps alive. On a
/// `false` return (invalid content, e.g. non-UTF-8) it reports `status = 2`
/// (`InvalidUtf8`). The thunk is reached as an *indirect* call, so the only
/// relocation the copied stencil carries is the `phon_cont` `BRANCH26`.
// r[impl compact.alignment]
#[no_mangle]
pub unsafe extern "C" fn phon_stencil_borrow(cx: *mut Ctx) {
    let c = &mut *cx;
    let info = &*(*c.prog as *const BorrowInfo);
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

    // Length-vs-remaining check: each element costs at least `stride.max(1)` bytes
    // (mirrors `read_len(stride.max(1))`), measured before padding.
    let remaining = (c.wire_end as usize) - (c.wire as usize);
    let min = if info.stride == 0 { 1 } else { info.stride };
    if count > remaining / min {
        c.status = 1;
        return;
    }

    // Pad before element bytes only when there is at least one element. An empty
    // run has no element storage, so the following field starts right after the
    // count, matching the interpreter.
    let pad = if count > 0 {
        let pos = (c.wire as usize) - (c.wire_start as usize);
        info.elem_align.wrapping_sub(pos & (info.elem_align - 1)) & (info.elem_align - 1)
    } else {
        0
    };
    let src = c.wire.add(pad);

    // The whole run must fit (the real bounds check for the borrowed slice).
    let total = count * info.stride;
    if (src as usize).wrapping_add(total) > c.wire_end as usize {
        c.status = 1;
        return;
    }

    // Construct the borrowed value at `base + field_offset`, pointing INTO the
    // input — NO alloc, NO copy. Indirect call through `info.set_borrowed`, so no
    // relocation. status 2 marks invalid content (e.g. non-UTF-8 `&str`).
    let field = c.base.add(info.field_offset);
    if !(info.set_borrowed)(info.thunks_ctx, field, src, count) {
        c.status = 2;
        return;
    }

    c.wire = src.add(total);
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

/// Decode a `Result<T, E>` into `base + field_offset`, then continue.
///
/// Reads a `u32` wire index, selects the matching Ok/Err arm, allocates scratch
/// for that arm payload, runs the arm body at `base = scratch` sharing the wire
/// cursor, then moves the arm into the result with `init_ok`/`init_err`. A wire
/// index matching neither arm rejects with `status = 4` (`BadVariantIndex`).
#[no_mangle]
pub unsafe extern "C" fn phon_stencil_result(cx: *mut Ctx) {
    let c = &mut *cx;
    let info = &*(*c.prog as *const ResultInfo);
    let outer_prog = c.prog.add(1);
    let outer_base = c.base;

    // Read the u32 arm index with a bounds check (like `read_u32`).
    if (c.wire as usize).wrapping_add(4) > c.wire_end as usize {
        c.status = 1;
        return;
    }
    let mut idx_bytes = [0u8; 4];
    core::ptr::copy_nonoverlapping(c.wire, idx_bytes.as_mut_ptr(), 4);
    let wire_index = u32::from_le_bytes(idx_bytes);
    c.wire = c.wire.add(4);

    let (size, align, arm_prog, arm_entry, init): (
        usize,
        usize,
        *const u64,
        unsafe extern "C" fn(*mut Ctx),
        unsafe extern "C" fn(*const (), *mut u8, *mut u8),
    ) = if wire_index == info.ok_wire_index {
        (
            info.ok_size,
            info.ok_align,
            info.ok_prog,
            info.ok_entry,
            info.init_ok,
        )
    } else if wire_index == info.err_wire_index {
        (
            info.err_size,
            info.err_align,
            info.err_prog,
            info.err_entry,
            info.init_err,
        )
    } else {
        c.status = 4;
        c.aux = wire_index as u64;
        return;
    };

    // Allocate scratch for the selected arm. size 0 -> dangling aligned.
    let (scratch, alloc_size) = if size == 0 {
        (align as *mut u8, 0usize)
    } else {
        let buf = (c.alloc)(size, align);
        if buf.is_null() {
            c.status = 1;
            return;
        }
        (buf, size)
    };

    // Run the selected arm body at `base = scratch`, sharing the wire cursor.
    c.prog = arm_prog;
    c.base = scratch;
    (arm_entry)(cx);
    if c.status != 0 {
        if alloc_size != 0 {
            (c.dealloc)(scratch, alloc_size, align);
        }
        return;
    }

    // Move the decoded arm into the result, then free scratch without dropping.
    c.base = outer_base;
    let result = outer_base.add(info.field_offset);
    (init)(info.thunks_ctx, result, scratch);
    if alloc_size != 0 {
        (c.dealloc)(scratch, alloc_size, align);
    }

    c.prog = outer_prog;

    #[cfg(tailcall)]
    become phon_cont(cx);
    #[cfg(not(tailcall))]
    phon_cont(cx);
}

/// Decode an opaque adapter field from a length-prefixed byte span, then continue.
///
/// The wire is `Primitive::Bytes`: a little-endian `u32` length and exactly that
/// many bytes, no padding. The adapter receives a borrowed pointer into the input
/// and initializes `base + field_offset`. A rejected adapter reports `status = 7`.
#[no_mangle]
pub unsafe extern "C" fn phon_stencil_opaque(cx: *mut Ctx) {
    let c = &mut *cx;
    let info = &*(*c.prog as *const OpaqueInfo);
    let outer_prog = c.prog.add(1);

    if (c.wire as usize).wrapping_add(4) > c.wire_end as usize {
        c.status = 1;
        return;
    }
    let mut len_bytes = [0u8; 4];
    core::ptr::copy_nonoverlapping(c.wire, len_bytes.as_mut_ptr(), 4);
    let len = u32::from_le_bytes(len_bytes) as usize;
    c.wire = c.wire.add(4);

    let remaining = (c.wire_end as usize) - (c.wire as usize);
    if len > remaining {
        c.status = 1;
        return;
    }

    let span = c.wire;
    let slot = c.base.add(info.field_offset);
    if !(info.decode)(info.thunks_ctx, span, len, slot) {
        c.status = 7;
        return;
    }

    c.wire = c.wire.add(len);
    c.prog = outer_prog;

    #[cfg(tailcall)]
    become phon_cont(cx);
    #[cfg(not(tailcall))]
    phon_cont(cx);
}

/// Decode one self-describing dynamic `Value`, then continue.
///
/// The value is self-delimiting, so the helper reports exactly how many input bytes
/// it consumed. On failure it writes the precise `DecodeError` through `Ctx.error`
/// and this stencil sets `status = 8`.
#[no_mangle]
pub unsafe extern "C" fn phon_stencil_dynamic(cx: *mut Ctx) {
    let c = &mut *cx;
    let info = &*(*c.prog as *const DynamicInfo);
    let outer_prog = c.prog.add(1);

    let remaining = (c.wire_end as usize) - (c.wire as usize);
    let mut consumed = 0usize;
    let slot = c.base.add(info.field_offset);
    if !(info.read)(c.wire, remaining, slot, &mut consumed, c.error) {
        c.status = 8;
        return;
    }

    c.wire = c.wire.add(consumed);
    c.prog = outer_prog;

    #[cfg(tailcall)]
    become phon_cont(cx);
    #[cfg(not(tailcall))]
    phon_cont(cx);
}

/// Call a precompiled recursive block at `base + offset`, then continue.
#[no_mangle]
pub unsafe extern "C" fn phon_stencil_callblock(cx: *mut Ctx) {
    let c = &mut *cx;
    let info = &*(*c.prog as *const CallBlockInfo);
    let outer_prog = c.prog.add(1);
    let outer_base = c.base;

    c.prog = info.prog;
    c.base = outer_base.add(info.offset);
    (info.entry)(cx);
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

/// Decode an owned map into `base + field_offset`, then continue.
///
/// Reads a `u32` entry count (bounds-checked like `read_len`),
/// `init_with_capacity(map, count)`, then loops `count` times: allocate a key
/// scratch (`key_size`/`key_align`; size 0 → dangling) and a value scratch, run the
/// key sub-chain at `base = key_scratch` then the value sub-chain at
/// `base = value_scratch` (sharing the wire cursor), `insert` (moving both in), and
/// free both scratch buffers WITHOUT dropping (ownership transferred). A mid-loop
/// sub-chain error frees the scratch and bails. After the loop, if
/// `len(map) != count` a key collapsed (duplicate): reject with `status = 6`
/// (`DuplicateKey`). Every call (sub-chain entries, thunks, alloc) is indirect, so
/// the only relocation a copied stencil carries is the `phon_cont` BRANCH26.
#[no_mangle]
pub unsafe extern "C" fn phon_stencil_map(cx: *mut Ctx) {
    let c = &mut *cx;
    let info = &*(*c.prog as *const MapInfo);
    let outer_prog = c.prog.add(1);
    let outer_base = c.base;

    // Read the u32 entry count with a bounds check (each entry costs at least 1
    // byte, like `read_len(1)` -> `read_u32`).
    if (c.wire as usize).wrapping_add(4) > c.wire_end as usize {
        c.status = 1;
        return;
    }
    let mut count_bytes = [0u8; 4];
    core::ptr::copy_nonoverlapping(c.wire, count_bytes.as_mut_ptr(), 4);
    let count = u32::from_le_bytes(count_bytes) as usize;
    c.wire = c.wire.add(4);

    // Length-vs-remaining check: each entry costs at least 1 wire byte
    // (mirrors `read_len(1)`).
    let remaining = (c.wire_end as usize) - (c.wire as usize);
    if count > remaining {
        c.status = 1;
        return;
    }

    // Initialize the (uninitialized) map with room for `count` entries. NOTE: a
    // decode error after this point leaks the partial map — the same
    // trivially-droppable limitation the interpreter documents.
    let map = outer_base.add(info.field_offset);
    (info.init_with_capacity)(info.thunks_ctx, map, count);

    let mut entry = 0;
    while entry < count {
        // Engine-owned scratch for the key and value. size 0 -> dangling aligned.
        let (key_scratch, key_alloc) = if info.key_size == 0 {
            (info.key_align as *mut u8, 0usize)
        } else {
            let buf = (c.alloc)(info.key_size, info.key_align);
            if buf.is_null() {
                c.status = 1;
                return;
            }
            (buf, info.key_size)
        };
        let (value_scratch, value_alloc) = if info.value_size == 0 {
            (info.value_align as *mut u8, 0usize)
        } else {
            let buf = (c.alloc)(info.value_size, info.value_align);
            if buf.is_null() {
                if key_alloc != 0 {
                    (c.dealloc)(key_scratch, key_alloc, info.key_align);
                }
                c.status = 1;
                return;
            }
            (buf, info.value_size)
        };

        // Run the key sub-chain at `base = key_scratch`, sharing the wire cursor.
        c.prog = info.key_prog;
        c.base = key_scratch;
        (info.key_entry)(cx);
        if c.status != 0 {
            if key_alloc != 0 {
                (c.dealloc)(key_scratch, key_alloc, info.key_align);
            }
            if value_alloc != 0 {
                (c.dealloc)(value_scratch, value_alloc, info.value_align);
            }
            return;
        }

        // Run the value sub-chain at `base = value_scratch`.
        c.prog = info.value_prog;
        c.base = value_scratch;
        (info.value_entry)(cx);
        if c.status != 0 {
            if key_alloc != 0 {
                (c.dealloc)(key_scratch, key_alloc, info.key_align);
            }
            if value_alloc != 0 {
                (c.dealloc)(value_scratch, value_alloc, info.value_align);
            }
            return;
        }

        // Move both into the map, then free the scratch WITHOUT dropping
        // (ownership transferred to the map).
        c.base = outer_base;
        (info.insert)(info.thunks_ctx, map, key_scratch, value_scratch);
        if key_alloc != 0 {
            (c.dealloc)(key_scratch, key_alloc, info.key_align);
        }
        if value_alloc != 0 {
            (c.dealloc)(value_scratch, value_alloc, info.value_align);
        }

        entry += 1;
    }

    // A repeated key collapses two entries into one — reject it (the dynamic
    // codec's `DuplicateKey`, the oracle). status 6 is mapped to `DuplicateKey`
    // in `run()`.
    if (info.len)(info.thunks_ctx, map) != count {
        c.status = 6;
        return;
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
        // No reader variant. Distinguish a variant the writer has but the reader
        // removed (writer-only -> status 5, `WriterOnlyVariant`) from a garbage
        // index in neither set (status 4, `BadVariantIndex`). Scan the writer-only
        // list by raw pointer (no indexing -> no `panic_bounds_check` libcall).
        c.aux = wire_index as u64;
        let mut j = 0usize;
        let mut writer_only = false;
        while j < info.writer_only_count {
            if *info.writer_only.add(j) == wire_index {
                writer_only = true;
                break;
            }
            j += 1;
        }
        c.status = if writer_only { 5 } else { 4 };
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

/// Write a reader-only field's default into `base + offset`, then continue.
///
/// Reads a `*const DefaultInfo` from `Ctx.prog`, calls its `thunk(ctx, base +
/// offset)` to initialize the field in place, and reads NO wire bytes. The thunk
/// is an *indirect* call through the info struct, so the only relocation the
/// copied stencil carries is the `phon_cont` `BRANCH26`. Decode-only — the
/// compatibility decision (this field is reader-only) was made at lowering.
/// (`r[compat.reader-only-fields]`.)
#[no_mangle]
pub unsafe extern "C" fn phon_stencil_default(cx: *mut Ctx) {
    let c = &mut *cx;
    let info = &*(*c.prog as *const DefaultInfo);
    c.prog = c.prog.add(1);

    // Initialize the reader field in place (no wire interaction). Indirect call
    // through `info.thunk` — no relocation.
    (info.thunk)(info.ctx, c.base.add(info.offset));

    #[cfg(tailcall)]
    become phon_cont(cx);
    #[cfg(not(tailcall))]
    phon_cont(cx);
}

/// Consume a writer-only value's wire bytes (writing nothing to memory), then
/// continue.
///
/// Reads a `*const SkipInfo` from `Ctx.prog`, calls
/// `walk(skip_op, wire_start, wire, wire_end)` to advance the cursor past one
/// writer value. A null return is a skip failure (truncation / bad presence byte
/// / unmatched enum index): reject with `status = 1`. Otherwise set `c.wire` to
/// the returned advanced cursor and continue. The walk is an *indirect* call
/// through the info struct, so the only relocation the copied stencil carries is
/// the `phon_cont` `BRANCH26`. Decode-only. (`r[compat.skip-writer-only]`.)
#[no_mangle]
pub unsafe extern "C" fn phon_stencil_skipwire(cx: *mut Ctx) {
    let c = &mut *cx;
    let info = &*(*c.prog as *const SkipInfo);
    let outer_prog = c.prog.add(1);

    // Advance the cursor over one writer value. Indirect call through `info.walk`
    // — no relocation. Null => the skip failed (truncated/malformed wire).
    let advanced = (info.walk)(info.skip_op, c.wire_start, c.wire, c.wire_end);
    if advanced.is_null() {
        c.status = 1;
        return;
    }
    c.wire = advanced;
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

/// An encode result op's immediates, reached through a `*const EncResultInfo` slot
/// in `EncCtx.prog`. Reads the active arm through result thunks, writes that arm's
/// wire index, then runs the matching sub-chain with `base` set to the arm value.
#[repr(C)]
pub struct EncResultInfo {
    /// Where the `Result<T, E>` handle lives, relative to `base`.
    pub field_offset: usize,
    /// Wire indices for the Ok and Err arms.
    pub ok_wire_index: u32,
    pub err_wire_index: u32,
    /// Opaque per-type context passed to the thunks.
    pub thunks_ctx: *const (),
    /// Whether the result is Ok.
    pub is_ok: unsafe extern "C" fn(ctx: *const (), result: *const u8) -> bool,
    /// Pointer to the Ok payload, valid only when Ok.
    pub get_ok: unsafe extern "C" fn(ctx: *const (), result: *const u8) -> *const u8,
    /// Pointer to the Err payload, valid only when Err.
    pub get_err: unsafe extern "C" fn(ctx: *const (), result: *const u8) -> *const u8,
    /// Entry/prog for the Ok arm body.
    pub ok_entry: unsafe extern "C" fn(cx: *mut EncCtx),
    pub ok_prog: *const u64,
    /// Entry/prog for the Err arm body.
    pub err_entry: unsafe extern "C" fn(cx: *mut EncCtx),
    pub err_prog: *const u64,
}

/// An encode opaque op's immediates, reached through a `*const EncOpaqueInfo` slot
/// in `EncCtx.prog`. The stencil frames the field as a `Primitive::Bytes` run and
/// delegates appending the inner bytes to the adapter thunk.
#[repr(C)]
pub struct EncOpaqueInfo {
    /// Where the opaque field lives, relative to `base`.
    pub field_offset: usize,
    /// Opaque per-field context passed to the thunk.
    pub thunks_ctx: *const (),
    /// Append the opaque inner bytes to the output `Vec<u8>`.
    pub encode: unsafe extern "C" fn(ctx: *const (), field: *const u8, out: *mut Vec<u8>),
}

/// An encode dynamic `Value` op's immediates, reached through a
/// `*const EncDynamicInfo` slot in `EncCtx.prog`.
#[repr(C)]
pub struct EncDynamicInfo {
    /// Where the `Value` lives, relative to `base`.
    pub field_offset: usize,
    /// Append the self-describing value bytes to the output `Vec<u8>`.
    pub write: unsafe extern "C" fn(value: *const u8, out: *mut Vec<u8>),
}

/// An encode recursive block call's immediates, reached through a
/// `*const EncCallBlockInfo` slot in `EncCtx.prog`.
#[repr(C)]
pub struct EncCallBlockInfo {
    /// Where the recursive value lives, relative to the current base.
    pub offset: usize,
    /// Entry/prog for the precompiled recursive block.
    pub entry: unsafe extern "C" fn(cx: *mut EncCtx),
    pub prog: *const u64,
}

/// An encode owned-map op's immediates, reached through a `*const EncMapInfo` slot
/// in `EncCtx.prog`. Mirrors `MapInfo` minus the decode-only init/insert thunks
/// (plus the stateful iterator thunks). The key sub-chain is run with
/// `base = k` and the value sub-chain with `base = v`, where `(k, v)` are the
/// borrowed pointers `iter_next` yields.
#[repr(C)]
pub struct EncMapInfo {
    /// Where the map handle lives, relative to `base`.
    pub field_offset: usize,
    /// Opaque per-type context passed to the thunks.
    pub thunks_ctx: *const (),
    /// The map's current entry count (written as the `u32` count prefix).
    pub len: unsafe extern "C" fn(ctx: *const (), map: *const u8) -> usize,
    /// Build a stateful iterator over the entries of the initialized map.
    pub iter_init: unsafe extern "C" fn(ctx: *const (), map: *const u8) -> *mut (),
    /// Advance the iterator, writing the next entry's borrowed key/value pointers
    /// to `key_out`/`value_out` and returning `true`, or `false` at the end.
    pub iter_next: unsafe extern "C" fn(
        ctx: *const (),
        iter: *mut (),
        key_out: *mut *const u8,
        value_out: *mut *const u8,
    ) -> bool,
    /// Free the iterator built by `iter_init`.
    pub iter_dealloc: unsafe extern "C" fn(ctx: *const (), iter: *mut ()),
    /// Entry to the key sub-chain (a `*mut EncCtx` function ending in `ret`).
    pub key_entry: unsafe extern "C" fn(cx: *mut EncCtx),
    /// The key sub-chain's immediate stream (reset into `EncCtx.prog`).
    pub key_prog: *const u64,
    /// Entry to the value sub-chain (a `*mut EncCtx` function ending in `ret`).
    pub value_entry: unsafe extern "C" fn(cx: *mut EncCtx),
    /// The value sub-chain's immediate stream (reset into `EncCtx.prog`).
    pub value_prog: *const u64,
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
// r[impl compact.alignment]
#[no_mangle]
pub unsafe extern "C" fn phon_stencil_bytes_enc(cx: *mut EncCtx) {
    let c = &mut *cx;
    let info = &*(*c.prog as *const EncBytesInfo);
    let outer_prog = c.prog.add(1);

    let list = c.base.add(info.field_offset);
    let count = (info.len)(info.thunks_ctx, list);
    let total = count * info.stride;

    // Write the u32 count (no alignment padding, like `write_u32`), then pad
    // before element bytes only when there is at least one element.
    let pad = if count > 0 {
        info.elem_align.wrapping_sub((c.out_pos + 4) & (info.elem_align - 1))
            & (info.elem_align - 1)
    } else {
        0
    };
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

/// Encode a `Result<T, E>` from `base + field_offset`, then continue.
///
/// Uses the bound result thunks to select Ok or Err, writes that arm's `u32` wire
/// index, then runs the matching arm body at the contained value pointer. The
/// arm body shares the output cursor through `EncCtx`.
#[no_mangle]
pub unsafe extern "C" fn phon_stencil_result_enc(cx: *mut EncCtx) {
    let c = &mut *cx;
    let info = &*(*c.prog as *const EncResultInfo);
    let outer_prog = c.prog.add(1);
    let outer_base = c.base;

    let result = outer_base.add(info.field_offset);
    let ok = (info.is_ok)(info.thunks_ctx, result);
    let (wire_index, arm_prog, arm_entry, arm_value) = if ok {
        (
            info.ok_wire_index,
            info.ok_prog,
            info.ok_entry,
            (info.get_ok)(info.thunks_ctx, result),
        )
    } else {
        (
            info.err_wire_index,
            info.err_prog,
            info.err_entry,
            (info.get_err)(info.thunks_ctx, result),
        )
    };

    // Write the u32 arm index (no alignment, like `write_u32`).
    let need = c.out_pos + 4;
    if need > c.out_cap {
        (c.grow)(cx, need);
    }
    let idx_bytes = wire_index.to_le_bytes();
    core::ptr::copy_nonoverlapping(idx_bytes.as_ptr(), c.out_ptr.add(c.out_pos), 4);
    c.out_pos += 4;

    // Run the selected arm body at the contained value pointer.
    c.prog = arm_prog;
    c.base = arm_value;
    (arm_entry)(cx);

    c.base = outer_base;
    c.prog = outer_prog;

    #[cfg(tailcall)]
    become phon_econt(cx);
    #[cfg(not(tailcall))]
    phon_econt(cx);
}

/// Encode an opaque adapter field as a length-prefixed byte span, then continue.
///
/// Synchronizes the raw output cursor into the backing `Vec`, lets the adapter
/// append inner bytes, refreshes the raw cursor after any reallocation, and
/// backpatches the `u32` byte length.
#[no_mangle]
pub unsafe extern "C" fn phon_stencil_opaque_enc(cx: *mut EncCtx) {
    let c = &mut *cx;
    let info = &*(*c.prog as *const EncOpaqueInfo);
    let outer_prog = c.prog.add(1);

    let len_pos = c.out_pos;
    let need = c.out_pos + 4;
    if need > c.out_cap {
        (c.grow)(cx, need);
    }
    core::ptr::write_bytes(c.out_ptr.add(len_pos), 0, 4);
    c.out_pos += 4;
    let start = c.out_pos;

    let out = &mut *c.out_handle.cast::<Vec<u8>>();
    out.set_len(c.out_pos);
    let field = c.base.add(info.field_offset);
    (info.encode)(info.thunks_ctx, field, c.out_handle.cast::<Vec<u8>>());
    let end = out.len();
    let inner_len = (end - start) as u32;

    c.out_ptr = out.as_mut_ptr();
    c.out_cap = out.capacity();
    c.out_pos = end;
    core::ptr::copy_nonoverlapping(inner_len.to_le_bytes().as_ptr(), c.out_ptr.add(len_pos), 4);

    c.prog = outer_prog;

    #[cfg(tailcall)]
    become phon_econt(cx);
    #[cfg(not(tailcall))]
    phon_econt(cx);
}

/// Encode one self-describing dynamic `Value`, then continue.
///
/// Like opaque encode, this synchronizes the raw output cursor into the backing
/// `Vec` before calling the helper, then refreshes the raw pointer/capacity after
/// any reallocation.
#[no_mangle]
pub unsafe extern "C" fn phon_stencil_dynamic_enc(cx: *mut EncCtx) {
    let c = &mut *cx;
    let info = &*(*c.prog as *const EncDynamicInfo);
    let outer_prog = c.prog.add(1);

    let out = &mut *c.out_handle.cast::<Vec<u8>>();
    out.set_len(c.out_pos);
    let value = c.base.add(info.field_offset);
    (info.write)(value, c.out_handle.cast::<Vec<u8>>());

    c.out_ptr = out.as_mut_ptr();
    c.out_cap = out.capacity();
    c.out_pos = out.len();
    c.prog = outer_prog;

    #[cfg(tailcall)]
    become phon_econt(cx);
    #[cfg(not(tailcall))]
    phon_econt(cx);
}

/// Call a precompiled recursive encode block at `base + offset`, then continue.
#[no_mangle]
pub unsafe extern "C" fn phon_stencil_callblock_enc(cx: *mut EncCtx) {
    let c = &mut *cx;
    let info = &*(*c.prog as *const EncCallBlockInfo);
    let outer_prog = c.prog.add(1);
    let outer_base = c.base;

    c.prog = info.prog;
    c.base = outer_base.add(info.offset);
    (info.entry)(cx);

    c.base = outer_base;
    c.prog = outer_prog;

    #[cfg(tailcall)]
    become phon_econt(cx);
    #[cfg(not(tailcall))]
    phon_econt(cx);
}

/// Encode an owned map from `base + field_offset`, then continue.
///
/// Reads the entry count via `len`, writes it as a `u32` (no alignment, like
/// `write_u32`), builds a stateful iterator via `iter_init`, then loops: `iter_next`
/// yields the next entry's borrowed key/value pointers (false → break); run the key
/// sub-chain at `base = k` then the value sub-chain at `base = v` (both reading
/// memory and appending to the shared output cursor). Finally `iter_dealloc`. Every
/// call is indirect, so the only relocation a copied stencil carries is the
/// `phon_econt` BRANCH26.
#[no_mangle]
pub unsafe extern "C" fn phon_stencil_map_enc(cx: *mut EncCtx) {
    let c = &mut *cx;
    let info = &*(*c.prog as *const EncMapInfo);
    let outer_prog = c.prog.add(1);
    let outer_base = c.base;

    let map = outer_base.add(info.field_offset);
    let count = (info.len)(info.thunks_ctx, map);

    // Write the u32 count (no alignment padding, like `write_u32`).
    let need = c.out_pos + 4;
    if need > c.out_cap {
        (c.grow)(cx, need);
    }
    let count_bytes = (count as u32).to_le_bytes();
    core::ptr::copy_nonoverlapping(count_bytes.as_ptr(), c.out_ptr.add(c.out_pos), 4);
    c.out_pos += 4;

    // Drive a stateful iterator over the entries, encoding each (key, value) pair
    // in turn. The sub-chains share the output cursor through `EncCtx`.
    let iter = (info.iter_init)(info.thunks_ctx, map);
    loop {
        let mut k: *const u8 = core::ptr::null();
        let mut v: *const u8 = core::ptr::null();
        if !(info.iter_next)(info.thunks_ctx, iter, &mut k, &mut v) {
            break;
        }
        // Key sub-chain at `base = k`.
        c.prog = info.key_prog;
        c.base = k;
        (info.key_entry)(cx);
        // Value sub-chain at `base = v`.
        c.prog = info.value_prog;
        c.base = v;
        (info.value_entry)(cx);
    }
    (info.iter_dealloc)(info.thunks_ctx, iter);

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
