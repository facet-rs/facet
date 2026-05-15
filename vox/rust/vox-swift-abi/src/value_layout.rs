//! Layout-driven Swift codec FFI (calibration surface only).
//!
//! See `notes/codec-architecture.md`. This module exposes only the
//! calibration-time entry points needed to produce a [`ValueLayout`] from
//! the Swift side. Hot-path byte writes are emitted by Swift directly
//! against Swift values using the layout's match/store patterns; no Rust
//! function is called per-store.
//!
//! Surface:
//! - `vox_swift_layout_arena_*` — opaque handle to a [`LayoutArena`] that
//!   owns variant arrays / field arrays / name bytes / patterns / nested
//!   layouts.
//! - `vox_swift_probe_two_variant_enum_v1` — given three pre-injected
//!   sample buffers (variant A with two distinct payloads, variant B),
//!   diff bytes to discover the discriminant pattern and Ok-payload
//!   offset, allocate a `ValueLayout`, return a stable pointer.
//!
//! The layout types themselves are `#[repr(C)]` and are mirrored as Swift
//! `@frozen` structs on the consumer side; nothing about reading or
//! writing values goes through this dylib at runtime.

pub use vox_jit_cal::value_layout::{
    BytePattern, FieldLayout, LayoutArena, LayoutBytes, PrimitiveKind, ValueLayout,
    ValueLayoutKind, VariantLayout,
};

use crate::{
    VOX_SWIFT_STATUS_BAD_ABI, VOX_SWIFT_STATUS_INVALID_BOOL, VOX_SWIFT_STATUS_OK,
    VOX_SWIFT_STATUS_UNEXPECTED_EOF, VOX_SWIFT_STATUS_UNSUPPORTED,
    VOX_SWIFT_STATUS_VARINT_OVERFLOW, vox_swift_owned_bytes, vox_swift_status_t,
};
use vox_jit_cal::postcard_codec;

/// Opaque handle to a [`LayoutArena`].
#[repr(C)]
pub struct vox_swift_layout_arena {
    _private: [u8; 0],
}

/// Create a fresh layout arena. Pair with
/// [`vox_swift_layout_arena_destroy_v1`] to release the storage and every
/// `*const ValueLayout` ever allocated through it.
///
/// # Safety
/// The returned pointer must be released exactly once with
/// [`vox_swift_layout_arena_destroy_v1`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_swift_layout_arena_create_v1() -> *mut vox_swift_layout_arena {
    let arena = Box::new(LayoutArena::new());
    Box::into_raw(arena).cast()
}

/// Release a layout arena and every layout / field / variant / name / pattern
/// allocated through it.
///
/// # Safety
/// `arena` must be null or a pointer returned by
/// `vox_swift_layout_arena_create_v1` that has not already been destroyed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_swift_layout_arena_destroy_v1(arena: *mut vox_swift_layout_arena) {
    if !arena.is_null() {
        unsafe {
            drop(Box::from_raw(arena.cast::<LayoutArena>()));
        }
    }
}

/// Probe a two-variant enum's layout from three pre-injected sample buffers
/// and allocate a [`ValueLayout`] in `arena` describing it.
///
/// The caller (typically Swift) is responsible for using its `inject`
/// witness to populate the three buffers before calling this:
///
/// - `variant_a_zero_bytes` — a sample of variant A whose payload byte
///   pattern is zero (or any baseline value).
/// - `variant_a_max_bytes` — a sample of variant A whose payload differs
///   from `variant_a_zero_bytes` in every byte that belongs to the
///   payload.
/// - `variant_b_zero_bytes` — a sample of variant B whose payload is zero
///   (or any value that doesn't accidentally collide with variant A's
///   bytes outside the discriminant region).
///
/// All three buffers must be exactly `value_size` bytes long.
///
/// `variant_a_field_layout` and `variant_b_field_layout` describe the
/// single payload field of each variant (this prototype only handles
/// single-payload-field variants; expand later). Pass null for variants
/// with no payload (e.g. an `err` case carrying nothing).
///
/// On success writes a `*const ValueLayout` pointing into the arena to
/// `out_layout` and returns `VOX_SWIFT_STATUS_OK`.
///
/// # Safety
/// All pointer arguments must be valid for the indicated reads/writes.
/// `arena` must be a live arena handle from `vox_swift_layout_arena_create_v1`.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn vox_swift_probe_two_variant_enum_v1(
    arena: *mut vox_swift_layout_arena,
    value_size: u32,
    value_align: u32,
    variant_a_zero_bytes: *const u8,
    variant_a_max_bytes: *const u8,
    variant_b_zero_bytes: *const u8,
    variant_a_name_ptr: *const u8,
    variant_a_name_len: usize,
    variant_a_field_layout: *const ValueLayout,
    variant_b_name_ptr: *const u8,
    variant_b_name_len: usize,
    variant_b_field_layout: *const ValueLayout,
    out_layout: *mut *const ValueLayout,
) -> vox_swift_status_t {
    if arena.is_null() || out_layout.is_null() {
        return VOX_SWIFT_STATUS_BAD_ABI;
    }
    if variant_a_zero_bytes.is_null()
        || variant_a_max_bytes.is_null()
        || variant_b_zero_bytes.is_null()
    {
        return VOX_SWIFT_STATUS_BAD_ABI;
    }
    if value_size == 0 {
        return VOX_SWIFT_STATUS_BAD_ABI;
    }

    let variant_a_name = LayoutBytes {
        ptr: variant_a_name_ptr,
        len: variant_a_name_len,
    };
    let variant_b_name = LayoutBytes {
        ptr: variant_b_name_ptr,
        len: variant_b_name_len,
    };

    let arena = unsafe { &*arena.cast::<LayoutArena>() };

    let n = value_size as usize;
    let a_zero = unsafe { std::slice::from_raw_parts(variant_a_zero_bytes, n) };
    let a_max = unsafe { std::slice::from_raw_parts(variant_a_max_bytes, n) };
    let b_zero = unsafe { std::slice::from_raw_parts(variant_b_zero_bytes, n) };

    // Variant A's payload byte range: bytes that differ between the two A
    // samples. The two share the same discriminant, so anything that
    // moved between them is payload.
    let mut payload_first: Option<usize> = None;
    let mut payload_last: Option<usize> = None;
    for i in 0..n {
        if a_zero[i] != a_max[i] {
            payload_first.get_or_insert(i);
            payload_last = Some(i);
        }
    }
    let Some(payload_first) = payload_first else {
        return VOX_SWIFT_STATUS_BAD_ABI;
    };
    let payload_last = payload_last.unwrap();
    let payload_end = payload_last + 1;
    let a_payload_offset = payload_first as u32;

    // Discriminant bytes: bytes that differ between A and B and lie
    // outside variant A's payload range. Build a per-variant
    // match/store pattern entry for each.
    let mut a_pattern_entries: Vec<BytePattern> = Vec::new();
    let mut b_pattern_entries: Vec<BytePattern> = Vec::new();
    for i in 0..n {
        if i >= payload_first && i < payload_end {
            continue;
        }
        if a_zero[i] != b_zero[i] {
            a_pattern_entries.push(BytePattern::full(i as u32, a_zero[i]));
            b_pattern_entries.push(BytePattern::full(i as u32, b_zero[i]));
        }
    }
    if a_pattern_entries.is_empty() {
        return VOX_SWIFT_STATUS_BAD_ABI;
    }

    // For an explicit-tag enum, match and store patterns coincide.
    let a_store_entries = a_pattern_entries.clone();
    let b_store_entries = b_pattern_entries.clone();

    let a_fields = if variant_a_field_layout.is_null() {
        Vec::new()
    } else {
        vec![FieldLayout {
            name: arena.alloc_str("0"),
            offset: a_payload_offset,
            _pad: 0,
            layout: variant_a_field_layout,
        }]
    };
    let (a_fields_ptr, a_field_count) = arena.alloc_fields(a_fields);

    let b_fields = if variant_b_field_layout.is_null() {
        Vec::new()
    } else {
        // Variant B's payload offset isn't independently probed by this
        // prototype; we conservatively reuse A's. A future probe variant
        // takes two B samples too and learns B's payload offset directly.
        vec![FieldLayout {
            name: arena.alloc_str("0"),
            offset: a_payload_offset,
            _pad: 0,
            layout: variant_b_field_layout,
        }]
    };
    let (b_fields_ptr, b_field_count) = arena.alloc_fields(b_fields);

    let (a_match_ptr, a_match_count) = arena.alloc_patterns(a_pattern_entries);
    let (a_store_ptr, a_store_count) = arena.alloc_patterns(a_store_entries);
    let (b_match_ptr, b_match_count) = arena.alloc_patterns(b_pattern_entries);
    let (b_store_ptr, b_store_count) = arena.alloc_patterns(b_store_entries);

    let variants = vec![
        VariantLayout {
            name: variant_a_name,
            match_pattern: a_match_ptr,
            match_pattern_count: a_match_count,
            store_pattern: a_store_ptr,
            store_pattern_count: a_store_count,
            fields: a_fields_ptr,
            field_count: a_field_count,
            _pad: 0,
        },
        VariantLayout {
            name: variant_b_name,
            match_pattern: b_match_ptr,
            match_pattern_count: b_match_count,
            store_pattern: b_store_ptr,
            store_pattern_count: b_store_count,
            fields: b_fields_ptr,
            field_count: b_field_count,
            _pad: 0,
        },
    ];
    let (variants_ptr, variant_count) = arena.alloc_variants(variants);

    let layout = ValueLayout {
        kind: ValueLayoutKind::Enum,
        size: value_size,
        align: value_align,
        primitive_kind: PrimitiveKind::Unit,
        fields: std::ptr::null(),
        field_count: 0,
        variants: variants_ptr,
        variant_count,
        opaque_handle: 0,
        _reserved: 0,
    };
    let layout_ptr = arena.alloc_layout(layout);

    unsafe { *out_layout = layout_ptr };
    VOX_SWIFT_STATUS_OK
}

/// Probe a niche-filled `Option<T>`-shaped enum from three pre-built
/// sample buffers. There is no separate discriminant region: the entire
/// value's bytes ARE the payload, and one variant (the "niche") is
/// recognised by an exact byte pattern; anything else is the
/// "catch-all" variant.
///
/// - `niche_variant_bytes` — bytes of a sample of the niche variant
///   (e.g. `None` of `Optional<UnsafeRawPointer>`, all zeroes).
/// - `catchall_a_bytes`, `catchall_b_bytes` — two distinct samples of
///   the catch-all variant. Required so the probe can verify the niche
///   pattern doesn't accidentally match a real catch-all.
/// - `*_name_*` — utf-8 names for the variants (just metadata; the
///   codec doesn't require any specific spelling).
/// - `catchall_field_layout` — the layout of the catch-all's payload
///   (the inner `T`); pass null if the catch-all has no payload.
///
/// On success writes the new `*const ValueLayout` to `out_layout`. The
/// niche variant is emitted *first* in the variant array so the codec's
/// match-first-then-fall-through dispatch picks it correctly.
///
/// # Safety
/// All pointer arguments must be valid for the indicated reads/writes.
/// `arena` must be a live arena handle.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn vox_swift_probe_option_niche_v1(
    arena: *mut vox_swift_layout_arena,
    value_size: u32,
    value_align: u32,
    niche_variant_bytes: *const u8,
    catchall_a_bytes: *const u8,
    catchall_b_bytes: *const u8,
    niche_name_ptr: *const u8,
    niche_name_len: usize,
    catchall_name_ptr: *const u8,
    catchall_name_len: usize,
    catchall_field_layout: *const ValueLayout,
    out_layout: *mut *const ValueLayout,
) -> vox_swift_status_t {
    if arena.is_null() || out_layout.is_null() {
        return VOX_SWIFT_STATUS_BAD_ABI;
    }
    if niche_variant_bytes.is_null() || catchall_a_bytes.is_null() || catchall_b_bytes.is_null() {
        return VOX_SWIFT_STATUS_BAD_ABI;
    }
    if value_size == 0 {
        return VOX_SWIFT_STATUS_BAD_ABI;
    }

    let arena = unsafe { &*arena.cast::<LayoutArena>() };
    let n = value_size as usize;

    // Re-use the inner core (probe_option_niche_layout) but build the
    // resulting variant names ourselves so the FFI layer doesn't depend
    // on a `LayoutBytes`-by-value API.
    let core = unsafe {
        vox_jit_cal::value_layout::probe_option_niche_layout(
            arena,
            n,
            value_align as usize,
            niche_variant_bytes,
            catchall_a_bytes,
            catchall_b_bytes,
            // The core probe writes generic "None"/"Some" strings into
            // its own arena allocations. Callers can override by reading
            // the layout and replacing variant names in-arena, but for
            // now we just use those defaults.
            ValueLayout::empty_opaque(),
        )
    };
    let layout = match core {
        Ok(l) => l,
        Err(_) => return VOX_SWIFT_STATUS_BAD_ABI,
    };

    // Replace the inner-T layout pointer on the catch-all with the one
    // the caller supplied (the core probe used a placeholder).
    if !catchall_field_layout.is_null() {
        let variants_ptr = layout.variants as *mut VariantLayout;
        // Variant 0 = niche (e.g. None); variant 1 = catch-all.
        let catchall = unsafe { &mut *variants_ptr.add(1) };
        let fields_ptr = catchall.fields as *mut FieldLayout;
        if !fields_ptr.is_null() && catchall.field_count > 0 {
            let f = unsafe { &mut *fields_ptr };
            f.layout = catchall_field_layout;
        }
    }

    // Replace variant names with what the caller passed.
    let niche_name = LayoutBytes {
        ptr: niche_name_ptr,
        len: niche_name_len,
    };
    let catchall_name = LayoutBytes {
        ptr: catchall_name_ptr,
        len: catchall_name_len,
    };
    let variants_ptr = layout.variants as *mut VariantLayout;
    if !variants_ptr.is_null() && layout.variant_count >= 2 {
        unsafe {
            (*variants_ptr).name = arena.alloc_str(niche_name.as_str().unwrap_or("None"));
            (*variants_ptr.add(1)).name = arena.alloc_str(catchall_name.as_str().unwrap_or("Some"));
        }
    }

    let layout_ptr = arena.alloc_layout(layout);
    unsafe { *out_layout = layout_ptr };
    VOX_SWIFT_STATUS_OK
}

/// Build a struct-shaped [`ValueLayout`] from explicit field info. Used
/// when the caller already knows field offsets (Swift can compute them
/// via `MemoryLayout<T>.offset(of:)`); no probing required.
///
/// Field info comes as four parallel arrays of length `field_count`:
/// names (utf-8 ptr+len pairs), offsets (absolute byte offsets within
/// the struct), and inner layout pointers. The builder allocates names
/// and a contiguous `FieldLayout` array in the arena, then a
/// `ValueLayout` referencing them, and returns the layout pointer
/// through `out_layout`.
///
/// # Safety
/// All array pointers must be valid for `field_count` elements; each
/// inner-layout pointer must be a live `*const ValueLayout`.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn vox_swift_make_struct_layout_v1(
    arena: *mut vox_swift_layout_arena,
    size: u32,
    align: u32,
    field_count: usize,
    field_name_ptrs: *const *const u8,
    field_name_lens: *const usize,
    field_offsets: *const u32,
    field_layouts: *const *const ValueLayout,
    out_layout: *mut *const ValueLayout,
) -> vox_swift_status_t {
    if arena.is_null() || out_layout.is_null() {
        return VOX_SWIFT_STATUS_BAD_ABI;
    }
    if field_count > 0
        && (field_name_ptrs.is_null()
            || field_name_lens.is_null()
            || field_offsets.is_null()
            || field_layouts.is_null())
    {
        return VOX_SWIFT_STATUS_BAD_ABI;
    }
    let arena = unsafe { &*arena.cast::<LayoutArena>() };

    let mut fields = Vec::with_capacity(field_count);
    for i in 0..field_count {
        let name_ptr = unsafe { *field_name_ptrs.add(i) };
        let name_len = unsafe { *field_name_lens.add(i) };
        let offset = unsafe { *field_offsets.add(i) };
        let layout_ptr = unsafe { *field_layouts.add(i) };
        if layout_ptr.is_null() {
            return VOX_SWIFT_STATUS_BAD_ABI;
        }
        let name_str = if name_ptr.is_null() || name_len == 0 {
            ""
        } else {
            let bytes = unsafe { std::slice::from_raw_parts(name_ptr, name_len) };
            std::str::from_utf8(bytes).unwrap_or("")
        };
        fields.push(FieldLayout {
            name: arena.alloc_str(name_str),
            layout: layout_ptr,
            offset,
            _pad: 0,
        });
    }

    let (fields_ptr, fc) = arena.alloc_fields(fields);
    let layout = ValueLayout {
        fields: fields_ptr,
        variants: std::ptr::null(),
        kind: ValueLayoutKind::Struct,
        size,
        align,
        primitive_kind: PrimitiveKind::Unit,
        field_count: fc,
        variant_count: 0,
        opaque_handle: 0,
        _reserved: 0,
    };
    let layout_ptr = arena.alloc_layout(layout);
    unsafe { *out_layout = layout_ptr };
    VOX_SWIFT_STATUS_OK
}

// ---------------------------------------------------------------------------
// Codec FFI: encode / decode a value through a calibrated ValueLayout.
//
// These are the only hot-path FFI entries. The byte stores themselves
// happen entirely inside the Rust dylib, walking the layout: the caller
// (Swift) just hands a value pointer in and gets bytes / a written value
// out.
// ---------------------------------------------------------------------------

fn codec_error_to_status(err: postcard_codec::CodecError) -> vox_swift_status_t {
    match err {
        postcard_codec::CodecError::Unsupported => VOX_SWIFT_STATUS_UNSUPPORTED,
        postcard_codec::CodecError::UnexpectedEof => VOX_SWIFT_STATUS_UNEXPECTED_EOF,
        postcard_codec::CodecError::VarintOverflow => VOX_SWIFT_STATUS_VARINT_OVERFLOW,
        postcard_codec::CodecError::InvalidBool => VOX_SWIFT_STATUS_INVALID_BOOL,
        postcard_codec::CodecError::NoMatchingVariant
        | postcard_codec::CodecError::InvalidVariantIndex => VOX_SWIFT_STATUS_BAD_ABI,
    }
}

/// Encode the value at `value_ptr` (matching `layout`) into a freshly
/// allocated buffer returned through `out_bytes`. The caller must
/// release the buffer with `vox_swift_owned_bytes_free_v1`.
///
/// # Safety
/// `layout` must be a live `*const ValueLayout`. `value_ptr` must point
/// to a valid value of the type described by the layout (at least
/// `layout.size` readable bytes). `out_bytes` must point to writable
/// storage for one `vox_swift_owned_bytes`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_swift_layout_encode_v1(
    layout: *const ValueLayout,
    value_ptr: *const u8,
    out_bytes: *mut vox_swift_owned_bytes,
) -> vox_swift_status_t {
    if layout.is_null() || value_ptr.is_null() || out_bytes.is_null() {
        return VOX_SWIFT_STATUS_BAD_ABI;
    }
    let layout = unsafe { &*layout };

    let mut buf: Vec<u8> = Vec::new();
    let result = unsafe { postcard_codec::encode_value(layout, value_ptr, &mut buf) };
    match result {
        Ok(()) => {
            // Hand the buffer's storage to the caller as
            // vox_swift_owned_bytes; release happens via
            // vox_swift_owned_bytes_free_v1.
            let mut buf = std::mem::ManuallyDrop::new(buf);
            unsafe {
                *out_bytes = vox_swift_owned_bytes {
                    ptr: buf.as_mut_ptr(),
                    len: buf.len(),
                    capacity: buf.capacity(),
                };
            }
            VOX_SWIFT_STATUS_OK
        }
        Err(e) => codec_error_to_status(e),
    }
}

/// Decode `input_len` bytes from `input_ptr` into the value-shaped
/// storage at `dst` (which must be writable for at least `layout.size`
/// bytes). Writes the number of input bytes consumed to
/// `out_consumed` (may be null).
///
/// # Safety
/// `layout` must be a live `*const ValueLayout`. `input_ptr`/`input_len`
/// must describe a readable byte slice. `dst` must point to writable
/// storage of at least `layout.size` bytes, suitably aligned.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_swift_layout_decode_v1(
    layout: *const ValueLayout,
    input_ptr: *const u8,
    input_len: usize,
    dst: *mut u8,
    out_consumed: *mut usize,
) -> vox_swift_status_t {
    if layout.is_null() || dst.is_null() {
        return VOX_SWIFT_STATUS_BAD_ABI;
    }
    if input_len > 0 && input_ptr.is_null() {
        return VOX_SWIFT_STATUS_BAD_ABI;
    }
    let layout = unsafe { &*layout };
    let input = if input_len == 0 {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(input_ptr, input_len) }
    };

    let result = unsafe { postcard_codec::decode_value(layout, input, dst) };
    match result {
        Ok(consumed) => {
            if !out_consumed.is_null() {
                unsafe { *out_consumed = consumed };
            }
            VOX_SWIFT_STATUS_OK
        }
        Err(e) => codec_error_to_status(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::MaybeUninit;
    use vox_jit_cal::value_layout::apply_store_pattern;

    /// Exercise the probe FFI against a Rust `Result<u64, ()>`. The same
    /// shape will be exercised from Swift: build three sample buffers via
    /// `inject` (or, for Rust, construct values natively), call the probe,
    /// read the `ValueLayout` it produces, and write `Ok(31)` from the
    /// caller side using only the layout's patterns/offsets — no Rust
    /// function is invoked to do the stores.
    #[test]
    fn ffi_probe_then_caller_writes_ok_31_directly() {
        let ok_zero: Result<u64, ()> = Ok(0);
        let ok_max: Result<u64, ()> = Ok(0xDEAD_BEEF_CAFE_BABE);
        let err_zero: Result<u64, ()> = Err(());
        let value_size = std::mem::size_of::<Result<u64, ()>>() as u32;
        let value_align = std::mem::align_of::<Result<u64, ()>>() as u32;

        let arena_handle = unsafe { vox_swift_layout_arena_create_v1() };
        let arena_ref = unsafe { &*arena_handle.cast::<LayoutArena>() };
        let u64_layout_ptr = arena_ref.alloc_layout(ValueLayout::primitive(PrimitiveKind::U64));
        let ok_name = b"Ok";
        let err_name = b"Err";

        let mut layout_ptr: *const ValueLayout = std::ptr::null();
        let status = unsafe {
            vox_swift_probe_two_variant_enum_v1(
                arena_handle,
                value_size,
                value_align,
                &ok_zero as *const _ as *const u8,
                &ok_max as *const _ as *const u8,
                &err_zero as *const _ as *const u8,
                ok_name.as_ptr(),
                ok_name.len(),
                u64_layout_ptr,
                err_name.as_ptr(),
                err_name.len(),
                std::ptr::null(),
                &mut layout_ptr,
            )
        };
        assert_eq!(status, VOX_SWIFT_STATUS_OK);
        assert!(!layout_ptr.is_null());

        let layout = unsafe { &*layout_ptr };
        assert_eq!(layout.kind, ValueLayoutKind::Enum);
        assert_eq!(layout.variant_count, 2);

        // The caller — playing the role Swift would play — writes Ok(31)
        // using only the layout's match/store patterns. No FFI helper.
        let variants = layout.variants_slice();
        let ok_variant = variants
            .iter()
            .find(|v| v.name.as_str() == Some("Ok"))
            .unwrap();
        let payload_offset = ok_variant.fields_slice()[0].offset as usize;

        let mut storage: MaybeUninit<Result<u64, ()>> = MaybeUninit::uninit();
        let dst = storage.as_mut_ptr() as *mut u8;
        unsafe {
            std::ptr::write_bytes(dst, 0, layout.size as usize);
            apply_store_pattern(ok_variant.store_pattern_slice(), dst);
            dst.add(payload_offset)
                .cast::<u64>()
                .write_unaligned(31_u64);
        }

        let result: Result<u64, ()> = unsafe { storage.assume_init() };
        assert_eq!(result, Ok(31));

        unsafe { vox_swift_layout_arena_destroy_v1(arena_handle) };
    }

    #[test]
    fn ffi_probe_rejects_null_args() {
        let mut out: *const ValueLayout = std::ptr::null();
        let dummy = 0_u8;

        let status = unsafe {
            vox_swift_probe_two_variant_enum_v1(
                std::ptr::null_mut(),
                8,
                8,
                &dummy,
                &dummy,
                &dummy,
                std::ptr::null(),
                0,
                std::ptr::null(),
                std::ptr::null(),
                0,
                std::ptr::null(),
                &mut out,
            )
        };
        assert_eq!(status, VOX_SWIFT_STATUS_BAD_ABI);
    }
}
