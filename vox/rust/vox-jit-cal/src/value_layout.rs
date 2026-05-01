//! Calibrated value-layout descriptors shared by every codec backend.
//!
//! Every type in this module is `#[repr(C)]` with a stable ABI: the same
//! `ValueLayout` graph can be produced by Rust calibration, by Swift
//! calibration, or by anything else, and codegen consumes it the same way.
//! Variable-length data (variant arrays, field arrays, names) lives behind
//! `(ptr, len)` slice pairs whose backing storage is owned by a
//! [`LayoutArena`] (in tests / build-time) or leaked for the process
//! (steady-state).
//!
//! Codegen reads a `ValueLayout` and emits direct stores. Per-type vtable
//! functions are not part of this representation — the probe that produces
//! it learns the offsets, alignments, and tag values once, and codegen
//! turns those numbers into `mov` instructions.

use std::cell::RefCell;
use std::fmt;
use std::ptr::NonNull;

/// Discriminant for [`ValueLayout`]. Values are part of the ABI.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueLayoutKind {
    Primitive = 0,
    Struct = 1,
    Enum = 2,
    /// Opaque container (Vec/String/Box/Swift Array/...). Layout is in the
    /// calibration registry under the [`DescriptorHandle`] in
    /// `opaque_handle`.
    Opaque = 3,
}

/// Fixed-size primitive scalar. Values are part of the ABI.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimitiveKind {
    Unit = 0,
    Bool = 1,
    U8 = 2,
    U16 = 3,
    U32 = 4,
    U64 = 5,
    I8 = 6,
    I16 = 7,
    I32 = 8,
    I64 = 9,
    F32 = 10,
    F64 = 11,
}

impl PrimitiveKind {
    pub const fn size(self) -> u32 {
        match self {
            Self::Bool | Self::U8 | Self::I8 => 1,
            Self::U16 | Self::I16 => 2,
            Self::U32 | Self::I32 | Self::F32 => 4,
            Self::U64 | Self::I64 | Self::F64 => 8,
            Self::Unit => 0,
        }
    }

    pub const fn align(self) -> u32 {
        match self.size() {
            0 => 1,
            n => n,
        }
    }
}

/// Borrowed UTF-8 bytes with FFI-stable layout. Identical shape to the
/// `vox_swift_bytes` struct used by `vox-swift-abi`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LayoutBytes {
    pub ptr: *const u8,
    pub len: usize,
}

impl LayoutBytes {
    pub const fn empty() -> Self {
        Self {
            ptr: std::ptr::null(),
            len: 0,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        if self.ptr.is_null() {
            return None;
        }
        let bytes = unsafe { std::slice::from_raw_parts(self.ptr, self.len) };
        std::str::from_utf8(bytes).ok()
    }
}

/// Layout of one value. Tagged-struct representation: `kind` selects which
/// of the trailing fields are meaningful. All fields are zero-initialized
/// for the kinds that don't use them.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ValueLayout {
    pub kind: ValueLayoutKind,
    /// Total size in bytes.
    pub size: u32,
    /// Alignment in bytes (power of two).
    pub align: u32,

    // --- kind == Primitive ---
    pub primitive_kind: PrimitiveKind,

    // --- kind == Struct ---
    pub fields: *const FieldLayout,
    pub field_count: u32,

    // --- kind == Enum ---
    pub tag_offset: u32,
    /// Width of the discriminant field in bytes. One of 1, 2, 4, 8.
    pub tag_width: u32,
    pub variants: *const VariantLayout,
    pub variant_count: u32,

    // --- kind == Opaque ---
    pub opaque_handle: u32,

    pub _reserved: u32,
}

impl ValueLayout {
    pub const fn empty_opaque() -> Self {
        Self {
            kind: ValueLayoutKind::Opaque,
            size: 0,
            align: 1,
            primitive_kind: PrimitiveKind::Unit,
            fields: std::ptr::null(),
            field_count: 0,
            tag_offset: 0,
            tag_width: 0,
            variants: std::ptr::null(),
            variant_count: 0,
            opaque_handle: 0,
            _reserved: 0,
        }
    }

    pub const fn primitive(kind: PrimitiveKind) -> Self {
        Self {
            kind: ValueLayoutKind::Primitive,
            size: kind.size(),
            align: kind.align(),
            primitive_kind: kind,
            fields: std::ptr::null(),
            field_count: 0,
            tag_offset: 0,
            tag_width: 0,
            variants: std::ptr::null(),
            variant_count: 0,
            opaque_handle: 0,
            _reserved: 0,
        }
    }

    pub fn fields_slice(&self) -> &[FieldLayout] {
        if self.fields.is_null() || self.field_count == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(self.fields, self.field_count as usize) }
        }
    }

    pub fn variants_slice(&self) -> &[VariantLayout] {
        if self.variants.is_null() || self.variant_count == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(self.variants, self.variant_count as usize) }
        }
    }
}

/// One field of a struct, or one piece of an enum-variant payload.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct FieldLayout {
    pub name: LayoutBytes,
    /// Byte offset from the base of the enclosing value. For variant fields
    /// this is absolute (within the entire enum value): it already accounts
    /// for the discriminant region.
    pub offset: u32,
    pub _pad: u32,
    pub layout: *const ValueLayout,
}

impl FieldLayout {
    pub fn layout(&self) -> &ValueLayout {
        unsafe { &*self.layout }
    }
}

/// One variant of an enum-shaped value.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct VariantLayout {
    pub name: LayoutBytes,
    /// Numeric tag value to write at the discriminant offset to select this
    /// variant.
    pub tag_value: u64,
    pub fields: *const FieldLayout,
    pub field_count: u32,
    pub _pad: u32,
}

impl VariantLayout {
    pub fn fields_slice(&self) -> &[FieldLayout] {
        if self.fields.is_null() || self.field_count == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(self.fields, self.field_count as usize) }
        }
    }
}

// ---------------------------------------------------------------------------
// Arena-style storage for variable-length pieces
// ---------------------------------------------------------------------------

/// Owns the variable-length backing storage referenced by a [`ValueLayout`]
/// graph (variant arrays, field arrays, name bytes, recursively-nested
/// `ValueLayout` nodes).
///
/// While the arena lives, every `*const` pointer it has handed out is valid
/// to dereference. Dropping the arena frees that storage. For process-wide
/// layouts you would leak the arena (`Box::leak`) or build with `Box::leak`
/// directly.
#[derive(Default)]
pub struct LayoutArena {
    inner: RefCell<ArenaInner>,
}

#[derive(Default)]
struct ArenaInner {
    layouts: Vec<Box<ValueLayout>>,
    fields: Vec<Box<[FieldLayout]>>,
    variants: Vec<Box<[VariantLayout]>>,
    names: Vec<Box<[u8]>>,
}

impl LayoutArena {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn alloc_str(&self, s: &str) -> LayoutBytes {
        let bytes: Box<[u8]> = s.as_bytes().to_vec().into_boxed_slice();
        let ptr = bytes.as_ptr();
        let len = bytes.len();
        self.inner.borrow_mut().names.push(bytes);
        LayoutBytes { ptr, len }
    }

    pub fn alloc_fields(&self, fields: Vec<FieldLayout>) -> (*const FieldLayout, u32) {
        if fields.is_empty() {
            return (std::ptr::null(), 0);
        }
        let len = fields.len() as u32;
        let boxed: Box<[FieldLayout]> = fields.into_boxed_slice();
        let ptr = boxed.as_ptr();
        self.inner.borrow_mut().fields.push(boxed);
        (ptr, len)
    }

    pub fn alloc_variants(&self, variants: Vec<VariantLayout>) -> (*const VariantLayout, u32) {
        if variants.is_empty() {
            return (std::ptr::null(), 0);
        }
        let len = variants.len() as u32;
        let boxed: Box<[VariantLayout]> = variants.into_boxed_slice();
        let ptr = boxed.as_ptr();
        self.inner.borrow_mut().variants.push(boxed);
        (ptr, len)
    }

    /// Move `layout` into the arena and return a stable pointer to it.
    pub fn alloc_layout(&self, layout: ValueLayout) -> *const ValueLayout {
        let boxed = Box::new(layout);
        let ptr = NonNull::from(boxed.as_ref()).as_ptr() as *const ValueLayout;
        self.inner.borrow_mut().layouts.push(boxed);
        ptr
    }
}

// SAFETY: the arena is interior-mutable but the borrowed slices it hands out
// are read-only from the consumer's perspective; the arena itself is never
// shared across threads (each backend builds its own).
impl fmt::Debug for LayoutArena {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let inner = self.inner.borrow();
        f.debug_struct("LayoutArena")
            .field("layouts", &inner.layouts.len())
            .field("fields", &inner.fields.len())
            .field("variants", &inner.variants.len())
            .field("names", &inner.names.len())
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Layout-driven byte writer (no per-type helper calls)
// ---------------------------------------------------------------------------

/// Write the discriminant bytes for variant `variant_index` into `dst`
/// (which points at the base of the enum value).
///
/// # Safety
/// `dst` must be writable for at least `layout.size` bytes. `layout.kind`
/// must be `Enum`. `variant_index` must be a valid index.
pub unsafe fn write_enum_tag(layout: &ValueLayout, dst: *mut u8, variant_index: u32) {
    debug_assert_eq!(layout.kind, ValueLayoutKind::Enum);
    let variant = &layout.variants_slice()[variant_index as usize];
    let tag_value = variant.tag_value;
    let tag_dst = unsafe { dst.add(layout.tag_offset as usize) };
    match layout.tag_width {
        1 => unsafe { tag_dst.cast::<u8>().write_unaligned(tag_value as u8) },
        2 => unsafe { tag_dst.cast::<u16>().write_unaligned(tag_value as u16) },
        4 => unsafe { tag_dst.cast::<u32>().write_unaligned(tag_value as u32) },
        8 => unsafe { tag_dst.cast::<u64>().write_unaligned(tag_value) },
        other => panic!("invalid tag_width {other}"),
    }
}

// ---------------------------------------------------------------------------
// Probe: build a ValueLayout for Result<T, E> by byte-comparing samples
// ---------------------------------------------------------------------------

/// Probe `Result<T, E>`'s in-memory layout into the given arena.
///
/// `t_zero` and `t_max` must be two `T` values whose byte representations
/// differ in at least one byte that is wholly within `T`. Likewise
/// `e_zero`/`e_max` for `E`. For ZST `E` the `e_*` arguments are unused.
///
/// Returns a [`ValueLayout`] (kind == Enum) whose backing storage lives in
/// `arena`.
pub fn probe_result_layout<T, E>(
    arena: &LayoutArena,
    t_zero: T,
    t_max: T,
    e_zero: E,
    _e_max: E,
    t_layout: ValueLayout,
    e_layout: ValueLayout,
) -> Result<ValueLayout, String>
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

    let tag_width: u32 = 1;
    let ok_tag = ok_zero_bytes[tag_offset] as u64;
    let err_tag = err_zero_bytes[tag_offset] as u64;

    let t_layout_ptr = arena.alloc_layout(t_layout);
    let e_layout_ptr = arena.alloc_layout(e_layout);

    let ok_field = FieldLayout {
        name: arena.alloc_str("0"),
        offset: ok_payload_offset as u32,
        _pad: 0,
        layout: t_layout_ptr,
    };
    let err_field = FieldLayout {
        name: arena.alloc_str("0"),
        offset: 0,
        _pad: 0,
        layout: e_layout_ptr,
    };

    let (ok_fields_ptr, ok_field_count) = arena.alloc_fields(vec![ok_field]);
    let (err_fields_ptr, err_field_count) = if size_of::<E>() == 0 {
        (std::ptr::null(), 0)
    } else {
        arena.alloc_fields(vec![err_field])
    };

    let variants = vec![
        VariantLayout {
            name: arena.alloc_str("Ok"),
            tag_value: ok_tag,
            fields: ok_fields_ptr,
            field_count: ok_field_count,
            _pad: 0,
        },
        VariantLayout {
            name: arena.alloc_str("Err"),
            tag_value: err_tag,
            fields: err_fields_ptr,
            field_count: err_field_count,
            _pad: 0,
        },
    ];
    let (variants_ptr, variant_count) = arena.alloc_variants(variants);

    Ok(ValueLayout {
        kind: ValueLayoutKind::Enum,
        size: size as u32,
        align: align as u32,
        primitive_kind: PrimitiveKind::Unit,
        fields: std::ptr::null(),
        field_count: 0,
        tag_offset: tag_offset as u32,
        tag_width,
        variants: variants_ptr,
        variant_count,
        opaque_handle: 0,
        _reserved: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::MaybeUninit;

    #[test]
    fn layout_driven_init_result_ok_31_u64() {
        let arena = LayoutArena::new();
        let layout = probe_result_layout(
            &arena,
            0_u64,
            0xDEAD_BEEF_CAFE_BABE_u64,
            (),
            (),
            ValueLayout::primitive(PrimitiveKind::U64),
            ValueLayout::primitive(PrimitiveKind::Unit),
        )
        .expect("probe Result<u64, ()>");

        assert_eq!(layout.kind, ValueLayoutKind::Enum);
        assert_eq!(layout.variant_count, 2);

        let mut storage: MaybeUninit<Result<u64, ()>> = MaybeUninit::uninit();
        let dst = storage.as_mut_ptr() as *mut u8;
        unsafe { std::ptr::write_bytes(dst, 0, layout.size as usize) };

        let variants = layout.variants_slice();
        let ok_index = variants
            .iter()
            .position(|v| v.name.as_str() == Some("Ok"))
            .unwrap() as u32;
        unsafe { write_enum_tag(&layout, dst, ok_index) };

        let payload_offset = variants[ok_index as usize].fields_slice()[0].offset as usize;
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
        let arena = LayoutArena::new();
        let layout = probe_result_layout(
            &arena,
            0_u64,
            0xDEAD_BEEF_CAFE_BABE_u64,
            (),
            (),
            ValueLayout::primitive(PrimitiveKind::U64),
            ValueLayout::primitive(PrimitiveKind::Unit),
        )
        .unwrap();

        let mut storage: MaybeUninit<Result<u64, ()>> = MaybeUninit::uninit();
        let dst = storage.as_mut_ptr() as *mut u8;
        unsafe { std::ptr::write_bytes(dst, 0, layout.size as usize) };

        let variants = layout.variants_slice();
        let err_index = variants
            .iter()
            .position(|v| v.name.as_str() == Some("Err"))
            .unwrap() as u32;
        unsafe { write_enum_tag(&layout, dst, err_index) };

        let result: Result<u64, ()> = unsafe { storage.assume_init() };
        assert_eq!(result, Err(()));
    }

    #[test]
    fn primitive_layout_constructs_correctly() {
        let layout = ValueLayout::primitive(PrimitiveKind::U32);
        assert_eq!(layout.kind, ValueLayoutKind::Primitive);
        assert_eq!(layout.size, 4);
        assert_eq!(layout.align, 4);
        assert_eq!(layout.primitive_kind, PrimitiveKind::U32);
    }
}
