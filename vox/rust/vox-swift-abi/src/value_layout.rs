//! Layout-driven Swift codec FFI (calibration surface only).
//!
//! See `notes/codec-architecture.md`. This module exposes only the
//! calibration-time entry points needed to produce a [`ValueLayout`] from
//! the Swift side. Hot-path byte writes are emitted by Swift directly
//! against Swift values using the layout's offsets/widths/tag values; no
//! Rust function is called per-store.
//!
//! Surface:
//! - `vox_swift_layout_arena_*` — opaque handle to a [`LayoutArena`] that
//!   owns variant arrays / field arrays / name bytes / nested layouts.
//! - `vox_swift_probe_two_variant_enum_v1` — given three pre-injected
//!   sample buffers (variant A with two distinct payloads, variant B), find
//!   the discriminant location and the variant payload offset, allocate a
//!   `ValueLayout` describing the enum, and return a stable pointer.
//!
//! The layout types themselves are `#[repr(C)]` and are mirrored as Swift
//! `@frozen` structs on the consumer side; nothing about reading or
//! writing values goes through this dylib at runtime.

pub use vox_jit_cal::value_layout::{
    FieldLayout, LayoutArena, LayoutBytes, PrimitiveKind, ValueLayout, ValueLayoutKind,
    VariantLayout,
};

use crate::{VOX_SWIFT_STATUS_BAD_ABI, VOX_SWIFT_STATUS_OK, vox_swift_status_t};

/// Opaque handle to a [`LayoutArena`].
#[repr(C)]
pub struct vox_swift_layout_arena {
    _private: [u8; 0],
}

/// Create a fresh layout arena. Pair with
/// [`vox_swift_layout_arena_destroy_v1`] to release the storage and every
/// `*const ValueLayout` ever allocated through it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_swift_layout_arena_create_v1() -> *mut vox_swift_layout_arena {
    let arena = Box::new(LayoutArena::new());
    Box::into_raw(arena).cast()
}

/// Release a layout arena and every layout / field / variant / name allocated
/// through it.
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
/// - `variant_a_zero_bytes` — a sample of variant A whose payload byte-pattern
///   is zero (or any baseline value).
/// - `variant_a_max_bytes` — a sample of variant A whose payload differs from
///   `variant_a_zero_bytes` in every byte that belongs to the payload.
/// - `variant_b_zero_bytes` — a sample of variant B whose payload is zero (or
///   any value that doesn't accidentally collide with variant A's bytes
///   outside the discriminant region).
///
/// All three buffers must be exactly `value_size` bytes long.
///
/// `variant_a_field_layout` and `variant_b_field_layout` describe the single
/// payload field of each variant (this prototype only handles
/// single-payload-field variants; expand later). Pass null for variants with
/// no payload (e.g. an `err` case carrying nothing).
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
    variant_a_name: LayoutBytes,
    variant_a_field_layout: *const ValueLayout,
    variant_b_name: LayoutBytes,
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

    let arena = unsafe { &*arena.cast::<LayoutArena>() };

    let n = value_size as usize;
    let a_zero = unsafe { std::slice::from_raw_parts(variant_a_zero_bytes, n) };
    let a_max = unsafe { std::slice::from_raw_parts(variant_a_max_bytes, n) };
    let b_zero = unsafe { std::slice::from_raw_parts(variant_b_zero_bytes, n) };

    // Find variant A's payload byte range: the bytes that differ between
    // `a_zero` and `a_max`. Both have the same discriminant (variant A),
    // so the only differences are within the payload.
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

    // Find the discriminant: a byte that differs between a_zero and b_zero
    // and lies outside the payload range above.
    let mut tag_offset: Option<usize> = None;
    for i in 0..n {
        if i >= payload_first && i < payload_end {
            continue;
        }
        if a_zero[i] != b_zero[i] {
            tag_offset = Some(i);
            break;
        }
    }
    let Some(tag_offset) = tag_offset else {
        return VOX_SWIFT_STATUS_BAD_ABI;
    };

    let tag_width: u32 = 1;
    let a_tag_value = a_zero[tag_offset] as u64;
    let b_tag_value = b_zero[tag_offset] as u64;

    // Build the variant fields.
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
        // Variant B's payload offset: probe by comparing two B samples — but
        // this prototype takes only one B sample, so we conservatively use
        // the same offset as A. A future extension takes two B samples too.
        vec![FieldLayout {
            name: arena.alloc_str("0"),
            offset: a_payload_offset,
            _pad: 0,
            layout: variant_b_field_layout,
        }]
    };
    let (b_fields_ptr, b_field_count) = arena.alloc_fields(b_fields);

    let variants = vec![
        VariantLayout {
            name: variant_a_name,
            tag_value: a_tag_value,
            fields: a_fields_ptr,
            field_count: a_field_count,
            _pad: 0,
        },
        VariantLayout {
            name: variant_b_name,
            tag_value: b_tag_value,
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
        tag_offset: tag_offset as u32,
        tag_width,
        variants: variants_ptr,
        variant_count,
        opaque_handle: 0,
        _reserved: 0,
    };
    let layout_ptr = arena.alloc_layout(layout);

    unsafe { *out_layout = layout_ptr };
    VOX_SWIFT_STATUS_OK
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::MaybeUninit;

    /// Exercise the probe FFI against a Rust `Result<u64, ()>`. The same
    /// shape will be exercised from Swift: build three sample buffers via
    /// `inject` (or, for Rust, construct values natively), call the probe,
    /// read the `ValueLayout` it produces, and write `Ok(31)` from the
    /// caller side using only the layout's offsets and tag values — no
    /// Rust function is invoked to do the stores.
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
        let ok_name = arena_ref.alloc_str("Ok");
        let err_name = arena_ref.alloc_str("Err");

        let mut layout_ptr: *const ValueLayout = std::ptr::null();
        let status = unsafe {
            vox_swift_probe_two_variant_enum_v1(
                arena_handle,
                value_size,
                value_align,
                &ok_zero as *const _ as *const u8,
                &ok_max as *const _ as *const u8,
                &err_zero as *const _ as *const u8,
                ok_name,
                u64_layout_ptr,
                err_name,
                std::ptr::null(),
                &mut layout_ptr,
            )
        };
        assert_eq!(status, VOX_SWIFT_STATUS_OK);
        assert!(!layout_ptr.is_null());

        let layout = unsafe { &*layout_ptr };
        assert_eq!(layout.kind, ValueLayoutKind::Enum);
        assert_eq!(layout.variant_count, 2);

        // The caller — playing the role the Swift codegen would play —
        // writes Ok(31) using only the integers in `layout`. There is no
        // FFI call to do the stores; we just `add` the offset and `store`
        // the constant.
        let variants = layout.variants_slice();
        let ok_index = variants
            .iter()
            .position(|v| v.name.as_str() == Some("Ok"))
            .unwrap();
        let ok_variant = &variants[ok_index];
        let payload_offset = ok_variant.fields_slice()[0].offset as usize;
        let tag_offset = layout.tag_offset as usize;
        let tag_value = ok_variant.tag_value as u8;
        assert_eq!(layout.tag_width, 1);

        let mut storage: MaybeUninit<Result<u64, ()>> = MaybeUninit::uninit();
        let dst = storage.as_mut_ptr() as *mut u8;
        unsafe {
            std::ptr::write_bytes(dst, 0, layout.size as usize);
            // Two stores. No call_indirect, no helper, no init_ok_fn.
            dst.add(tag_offset).cast::<u8>().write_unaligned(tag_value);
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
        let name = LayoutBytes::empty();

        let status = unsafe {
            vox_swift_probe_two_variant_enum_v1(
                std::ptr::null_mut(),
                8,
                8,
                &dummy,
                &dummy,
                &dummy,
                name,
                std::ptr::null(),
                name,
                std::ptr::null(),
                &mut out,
            )
        };
        assert_eq!(status, VOX_SWIFT_STATUS_BAD_ABI);
    }
}
