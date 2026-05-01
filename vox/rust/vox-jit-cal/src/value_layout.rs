//! Calibrated value-layout descriptors shared by every codec backend.
//!
//! These types describe a concrete in-memory layout — enough to construct a
//! valid value of the type by writing bytes at known offsets, with no help
//! from per-type vtable functions. Every backend (Rust JIT, Swift codec)
//! produces the same `ValueLayout` shape from its own source of truth (facet
//! shapes for Rust, `vox_swift_type_descriptor` for Swift). Codegen then
//! consumes a `ValueLayout` and emits direct stores.
//!
//! The first concrete case driving the design is "initialize a
//! `Result<u64, _>` as `Ok(31)`": the layout must give us the discriminant
//! offset and width, the discriminant value for `Ok`, and the offset of the
//! payload `u64`. With those three numbers the codegen emits two stores —
//! no `init_ok_fn` call.

use std::fmt;

/// Width of an enum-style discriminant in bytes. Must be 1, 2, 4, or 8.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiscriminantWidth {
    U8 = 1,
    U16 = 2,
    U32 = 4,
    U64 = 8,
}

impl DiscriminantWidth {
    #[inline]
    pub fn bytes(self) -> usize {
        self as usize
    }

    pub fn from_bytes(bytes: usize) -> Option<Self> {
        Some(match bytes {
            1 => Self::U8,
            2 => Self::U16,
            4 => Self::U32,
            8 => Self::U64,
            _ => return None,
        })
    }
}

/// Fixed-size primitive layout (the bytes are written directly; no helper).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimitiveKind {
    Bool,
    U8,
    U16,
    U32,
    U64,
    I8,
    I16,
    I32,
    I64,
    F32,
    F64,
    Unit,
}

impl PrimitiveKind {
    pub fn size(self) -> usize {
        match self {
            Self::Bool | Self::U8 | Self::I8 => 1,
            Self::U16 | Self::I16 => 2,
            Self::U32 | Self::I32 | Self::F32 => 4,
            Self::U64 | Self::I64 | Self::F64 => 8,
            Self::Unit => 0,
        }
    }

    pub fn align(self) -> usize {
        self.size().max(1)
    }
}

/// Description of one field within a struct or one piece of a variant payload.
#[derive(Clone, Debug)]
pub struct FieldLayout {
    pub name: String,
    /// Byte offset from the base of the enclosing value. For variant fields
    /// this is absolute (within the entire enum value) — i.e. it already
    /// accounts for the discriminant region.
    pub offset: usize,
    pub layout: ValueLayout,
}

/// Description of one variant of an enum-shaped value.
#[derive(Clone, Debug)]
pub struct VariantLayout {
    pub name: String,
    /// Numeric tag value written at the discriminant offset to select this
    /// variant. Always non-negative even for signed-discriminant Rust enums;
    /// the codegen widens to the appropriate `DiscriminantWidth`.
    pub tag_value: u64,
    /// Fields belonging to this variant. May be empty for unit variants.
    pub fields: Vec<FieldLayout>,
}

/// A calibrated enum/option/result layout.
#[derive(Clone, Debug)]
pub struct EnumLayout {
    pub size: usize,
    pub align: usize,
    pub tag_offset: usize,
    pub tag_width: DiscriminantWidth,
    pub variants: Vec<VariantLayout>,
}

impl EnumLayout {
    /// Find a variant by name (linear scan).
    pub fn variant_by_name(&self, name: &str) -> Option<&VariantLayout> {
        self.variants.iter().find(|v| v.name == name)
    }
}

/// Description of a struct's fields.
#[derive(Clone, Debug)]
pub struct StructLayout {
    pub size: usize,
    pub align: usize,
    pub fields: Vec<FieldLayout>,
}

/// Recursive value-layout description.
#[derive(Clone, Debug)]
pub enum ValueLayout {
    Primitive {
        kind: PrimitiveKind,
    },
    Struct(StructLayout),
    Enum(EnumLayout),
    /// A fully-opaque container described by an [`OpaqueDescriptor`] handle
    /// in the calibration registry. Reserved for `Vec`, `String`, `Box`, and
    /// the Swift equivalents — anything whose internal layout we deliberately
    /// don't reach into.
    Opaque {
        descriptor: super::DescriptorHandle,
    },
}

impl fmt::Display for ValueLayout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Primitive { kind } => write!(f, "{kind:?}"),
            Self::Struct(s) => write!(f, "Struct(size={}, fields={})", s.size, s.fields.len()),
            Self::Enum(e) => write!(
                f,
                "Enum(size={}, tag_offset={}, variants={})",
                e.size,
                e.tag_offset,
                e.variants.len()
            ),
            Self::Opaque { descriptor } => write!(f, "Opaque(handle={:?})", descriptor),
        }
    }
}

// ---------------------------------------------------------------------------
// Layout-driven byte writers (no per-type helper calls)
// ---------------------------------------------------------------------------

/// Write the discriminant bytes for variant `variant_index` of `layout` into
/// `dst` (which points at the base of the enum value).
///
/// # Safety
/// `dst` must be writable for at least `layout.size` bytes and aligned to
/// `layout.align`. `variant_index` must be a valid index into
/// `layout.variants`.
pub unsafe fn write_enum_tag(layout: &EnumLayout, dst: *mut u8, variant_index: usize) {
    let tag_value = layout.variants[variant_index].tag_value;
    let tag_dst = unsafe { dst.add(layout.tag_offset) };
    match layout.tag_width {
        DiscriminantWidth::U8 => unsafe { tag_dst.cast::<u8>().write_unaligned(tag_value as u8) },
        DiscriminantWidth::U16 => unsafe {
            tag_dst.cast::<u16>().write_unaligned(tag_value as u16)
        },
        DiscriminantWidth::U32 => unsafe {
            tag_dst.cast::<u32>().write_unaligned(tag_value as u32)
        },
        DiscriminantWidth::U64 => unsafe { tag_dst.cast::<u64>().write_unaligned(tag_value) },
    }
}

/// Probe the in-memory layout of `Result<T, E>` by constructing canonical
/// values and comparing their bytes.
///
/// Works for `T`, `E` whose layout is fully known to the caller — this
/// helper only learns the discriminant placement and the variant payload
/// offsets. For `Result<u64, ()>` (the first driver) the probe is sufficient
/// to construct an `Ok(_)` value with a single discriminant store and a
/// single u64 store.
///
/// `t_zero` and `t_max` must be two distinct `T` values whose byte
/// representations differ in at least one byte that is wholly within `T`.
/// Likewise `e_zero` and `e_max` for `E`. For ZST `E` the `e_*` arguments
/// are unused; pass `()` (which is what `probe_result_layout` accepts).
pub fn probe_result_layout<T, E>(
    t_zero: T,
    t_max: T,
    e_zero: E,
    _e_max: E,
    t_layout: ValueLayout,
    e_layout: ValueLayout,
) -> Result<EnumLayout, String>
where
    T: Copy,
    E: Copy,
{
    use std::mem::{align_of, size_of};

    let size = size_of::<Result<T, E>>();
    let align = align_of::<Result<T, E>>();

    let ok_zero: Result<T, E> = Ok(t_zero);
    let ok_max: Result<T, E> = Ok(t_max);
    let err_zero: Result<T, E> = Err(e_zero);

    let ok_zero_bytes =
        unsafe { std::slice::from_raw_parts(&ok_zero as *const _ as *const u8, size) };
    let ok_max_bytes =
        unsafe { std::slice::from_raw_parts(&ok_max as *const _ as *const u8, size) };
    let err_zero_bytes =
        unsafe { std::slice::from_raw_parts(&err_zero as *const _ as *const u8, size) };

    // Find the byte range that holds the Ok payload: the bytes that differ
    // between Ok(t_zero) and Ok(t_max).
    let mut ok_payload_first: Option<usize> = None;
    let mut ok_payload_last: Option<usize> = None;
    for i in 0..size {
        if ok_zero_bytes[i] != ok_max_bytes[i] {
            ok_payload_first.get_or_insert(i);
            ok_payload_last = Some(i);
        }
    }

    let ok_payload_offset = ok_payload_first.ok_or_else(|| {
        "probe failed: t_zero and t_max have identical byte representations".to_string()
    })?;
    let ok_payload_end = ok_payload_last.unwrap() + 1;

    // Find the discriminant: a byte that differs between Ok(_) and Err(_)
    // and lies outside the Ok payload range.
    let mut tag_offset: Option<usize> = None;
    for i in 0..size {
        if i >= ok_payload_offset && i < ok_payload_end {
            continue;
        }
        if ok_zero_bytes[i] != err_zero_bytes[i] {
            tag_offset = Some(i);
            break;
        }
    }
    let tag_offset = tag_offset.ok_or_else(|| {
        "probe failed: no byte outside Ok payload differs between Ok and Err".to_string()
    })?;

    // For now we only handle 1-byte discriminants (covers Result<u64, ()>
    // and Option<NonZero> with explicit reprs). Widen later if needed.
    let tag_width = DiscriminantWidth::U8;
    let ok_tag = ok_zero_bytes[tag_offset] as u64;
    let err_tag = err_zero_bytes[tag_offset] as u64;

    // The Err payload offset for ZST E is informally 0 — there are no bytes
    // to write. For non-ZST E we'd need a separate probe; the caller must
    // pass a non-ZST e_zero/e_max in that case. For now, accept ZST only.
    let err_payload_offset = if size_of::<E>() == 0 { 0 } else { 0 };

    Ok(EnumLayout {
        size,
        align,
        tag_offset,
        tag_width,
        variants: vec![
            VariantLayout {
                name: "Ok".to_string(),
                tag_value: ok_tag,
                fields: vec![FieldLayout {
                    name: "0".to_string(),
                    offset: ok_payload_offset,
                    layout: t_layout,
                }],
            },
            VariantLayout {
                name: "Err".to_string(),
                tag_value: err_tag,
                fields: vec![FieldLayout {
                    name: "0".to_string(),
                    offset: err_payload_offset,
                    layout: e_layout,
                }],
            },
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::MaybeUninit;

    #[test]
    fn layout_driven_init_result_ok_31_u64() {
        // Step 1: probe the layout. The probe gives us tag_offset, tag_width,
        // tag_value(Ok), and the offset of the u64 payload — nothing more.
        let layout = probe_result_layout(
            0_u64,
            0xDEAD_BEEF_CAFE_BABE_u64,
            (),
            (),
            ValueLayout::Primitive {
                kind: PrimitiveKind::U64,
            },
            ValueLayout::Primitive {
                kind: PrimitiveKind::Unit,
            },
        )
        .expect("probe Result<u64, ()>");

        // Step 2: directly write the bytes for Ok(31) using only those numbers.
        // No vtable function is called: we store the discriminant byte and the
        // u64 payload at calibrated offsets.
        let mut storage: MaybeUninit<Result<u64, ()>> = MaybeUninit::uninit();
        let dst = storage.as_mut_ptr() as *mut u8;

        // First, zero the storage so any padding bytes are stable. The probe
        // doesn't tell us about padding bytes; in real codegen we'd materialize
        // a full template. For this proof we're only writing the discriminant
        // and payload, which is enough for `Result::eq` since std drops
        // padding bytes when matching.
        unsafe { std::ptr::write_bytes(dst, 0, layout.size) };

        let ok_index = layout.variants.iter().position(|v| v.name == "Ok").unwrap();
        unsafe { write_enum_tag(&layout, dst, ok_index) };

        let payload_offset = layout.variants[ok_index].fields[0].offset;
        unsafe {
            dst.add(payload_offset)
                .cast::<u64>()
                .write_unaligned(31_u64)
        };

        let result: Result<u64, ()> = unsafe { storage.assume_init() };
        assert_eq!(result, Ok(31));
    }

    #[test]
    fn layout_driven_init_result_err_unit() {
        let layout = probe_result_layout(
            0_u64,
            0xDEAD_BEEF_CAFE_BABE_u64,
            (),
            (),
            ValueLayout::Primitive {
                kind: PrimitiveKind::U64,
            },
            ValueLayout::Primitive {
                kind: PrimitiveKind::Unit,
            },
        )
        .unwrap();

        let mut storage: MaybeUninit<Result<u64, ()>> = MaybeUninit::uninit();
        let dst = storage.as_mut_ptr() as *mut u8;
        unsafe { std::ptr::write_bytes(dst, 0, layout.size) };

        let err_index = layout
            .variants
            .iter()
            .position(|v| v.name == "Err")
            .unwrap();
        unsafe { write_enum_tag(&layout, dst, err_index) };
        // No payload bytes to write for Err(()).

        let result: Result<u64, ()> = unsafe { storage.assume_init() };
        assert_eq!(result, Err(()));
    }
}
