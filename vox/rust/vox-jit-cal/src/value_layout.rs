//! Calibrated value-layout descriptors shared by every codec backend.
//!
//! Every type in this module is `#[repr(C)]` with a stable ABI: the same
//! `ValueLayout` graph can be produced by Rust calibration, by Swift
//! calibration, or by anything else, and codegen consumes it the same way.
//! Variable-length data (variant arrays, field arrays, names, byte
//! patterns) lives behind `(ptr, len)` slice pairs whose backing storage
//! is owned by a [`LayoutArena`] (in tests / build-time) or leaked for
//! the process (steady-state).
//!
//! Codegen reads a `ValueLayout` and emits direct stores / loads.
//! Per-type vtable functions are not part of this representation — the
//! probe that produces it learns the patterns once, and codegen turns
//! those bytes into `mov` and `cmp` instructions.

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
    /// calibration registry under the handle in `opaque_handle`.
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

/// One byte of a variant's match or store pattern.
///
/// Both kinds of patterns describe a sequence of bytes within the enum
/// value. For a `match_pattern` entry, the byte at `offset` must satisfy
/// `(value_at_offset & mask) == (value & mask)` to count as a match. For
/// a `store_pattern` entry, codegen stores the bits selected by `mask`
/// from `value` into the byte at `offset`, leaving other bits intact (in
/// practice almost every byte uses `mask == 0xFF`, which collapses to a
/// plain store).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BytePattern {
    pub offset: u32,
    pub value: u8,
    pub mask: u8,
    pub _reserved: u16,
}

impl BytePattern {
    /// Convenience: a full-byte pattern (mask = 0xFF) at the given offset.
    pub const fn full(offset: u32, value: u8) -> Self {
        Self {
            offset,
            value,
            mask: 0xFF,
            _reserved: 0,
        }
    }
}

/// Layout of one value. Tagged-struct representation: `kind` selects which
/// of the trailing fields are meaningful.
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
    /// Byte offset from the base of the enclosing value. For variant
    /// fields this is absolute (within the entire enum value).
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
///
/// The variant is selected by `match_pattern`: every byte described in
/// the pattern must match. An *empty* `match_pattern` makes the variant
/// the "default" / catch-all — it matches anything no preceding variant
/// matched, which is how niche-filled `Some(_)` is encoded.
///
/// To construct this variant, codegen emits the stores described by
/// `store_pattern` and then the stores for each `field` in `fields`. For
/// niche-filled variants where the payload bytes themselves *are* the
/// discriminant (e.g. a non-null pointer makes a `Some`), `store_pattern`
/// can be empty — the field stores do all the work.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct VariantLayout {
    pub name: LayoutBytes,
    pub match_pattern: *const BytePattern,
    pub match_pattern_count: u32,
    pub store_pattern: *const BytePattern,
    pub store_pattern_count: u32,
    pub fields: *const FieldLayout,
    pub field_count: u32,
    pub _pad: u32,
}

impl VariantLayout {
    pub fn match_pattern_slice(&self) -> &[BytePattern] {
        if self.match_pattern.is_null() || self.match_pattern_count == 0 {
            &[]
        } else {
            unsafe {
                std::slice::from_raw_parts(self.match_pattern, self.match_pattern_count as usize)
            }
        }
    }

    pub fn store_pattern_slice(&self) -> &[BytePattern] {
        if self.store_pattern.is_null() || self.store_pattern_count == 0 {
            &[]
        } else {
            unsafe {
                std::slice::from_raw_parts(self.store_pattern, self.store_pattern_count as usize)
            }
        }
    }

    pub fn fields_slice(&self) -> &[FieldLayout] {
        if self.fields.is_null() || self.field_count == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(self.fields, self.field_count as usize) }
        }
    }

    /// Returns `true` if this variant has no `match_pattern`, i.e. it's
    /// the default / catch-all variant (must be last in the variant list
    /// for the dispatch to make sense).
    pub fn is_default(&self) -> bool {
        self.match_pattern_count == 0
    }
}

// ---------------------------------------------------------------------------
// Arena-style storage for variable-length pieces
// ---------------------------------------------------------------------------

/// Owns the variable-length backing storage referenced by a [`ValueLayout`]
/// graph (variant arrays, field arrays, name bytes, byte patterns,
/// recursively-nested `ValueLayout` nodes).
#[derive(Default)]
pub struct LayoutArena {
    inner: RefCell<ArenaInner>,
}

#[derive(Default)]
struct ArenaInner {
    layouts: Vec<Box<ValueLayout>>,
    fields: Vec<Box<[FieldLayout]>>,
    variants: Vec<Box<[VariantLayout]>>,
    patterns: Vec<Box<[BytePattern]>>,
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

    pub fn alloc_patterns(&self, patterns: Vec<BytePattern>) -> (*const BytePattern, u32) {
        if patterns.is_empty() {
            return (std::ptr::null(), 0);
        }
        let len = patterns.len() as u32;
        let boxed: Box<[BytePattern]> = patterns.into_boxed_slice();
        let ptr = boxed.as_ptr();
        self.inner.borrow_mut().patterns.push(boxed);
        (ptr, len)
    }

    pub fn alloc_layout(&self, layout: ValueLayout) -> *const ValueLayout {
        let boxed = Box::new(layout);
        let ptr = NonNull::from(boxed.as_ref()).as_ptr() as *const ValueLayout;
        self.inner.borrow_mut().layouts.push(boxed);
        ptr
    }
}

impl fmt::Debug for LayoutArena {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let inner = self.inner.borrow();
        f.debug_struct("LayoutArena")
            .field("layouts", &inner.layouts.len())
            .field("fields", &inner.fields.len())
            .field("variants", &inner.variants.len())
            .field("patterns", &inner.patterns.len())
            .field("names", &inner.names.len())
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Pattern application
// ---------------------------------------------------------------------------

/// Apply a `store_pattern` to `dst`: for each entry, write
/// `(byte & !mask) | (value & mask)` at `dst.add(offset)`. For full-byte
/// patterns (`mask == 0xFF`) this is a plain store.
///
/// # Safety
/// `dst` must be writable for at least `max(offset) + 1` bytes across
/// every entry.
pub unsafe fn apply_store_pattern(pattern: &[BytePattern], dst: *mut u8) {
    for entry in pattern {
        let p = unsafe { dst.add(entry.offset as usize) };
        if entry.mask == 0xFF {
            unsafe { p.write(entry.value) };
        } else {
            let existing = unsafe { p.read() };
            unsafe { p.write((existing & !entry.mask) | (entry.value & entry.mask)) };
        }
    }
}

/// Test whether `bytes` satisfy `match_pattern`: every entry's byte at
/// `offset`, masked, must equal `value` masked. An empty pattern is
/// treated as "always matches" (default variant).
///
/// # Safety
/// `bytes` must be readable for at least `max(offset) + 1` bytes.
pub unsafe fn matches_pattern(pattern: &[BytePattern], bytes: *const u8) -> bool {
    if pattern.is_empty() {
        return true;
    }
    for entry in pattern {
        let actual = unsafe { bytes.add(entry.offset as usize).read() };
        if (actual & entry.mask) != (entry.value & entry.mask) {
            return false;
        }
    }
    true
}

// ---------------------------------------------------------------------------
// Probe: build a ValueLayout for Result<T, E> by byte-comparing samples
// ---------------------------------------------------------------------------

/// Probe `Result<T, E>`'s in-memory layout into the given arena.
///
/// `t_zero` and `t_max` must be two `T` values whose byte representations
/// differ in at least one byte that is wholly within `T`. The discriminant
/// must live somewhere in the value that doesn't overlap with the Ok
/// payload (the common case for `#[repr(...)]` Rust enums and explicit
/// Swift enums; not the case for niche-filled `Option<Box<T>>` etc.,
/// which need a different probe).
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

    // Find the Ok payload byte range: bytes that differ between the two
    // Ok samples (their discriminants are equal so only the payload moves).
    let mut ok_payload_first: Option<usize> = None;
    let mut ok_payload_last: Option<usize> = None;
    for i in 0..size {
        if ok_zero_bytes[i] != ok_max_bytes[i] {
            ok_payload_first.get_or_insert(i);
            ok_payload_last = Some(i);
        }
    }
    let Some(ok_payload_first) = ok_payload_first else {
        return Err("probe failed: t_zero and t_max bytes are identical".to_string());
    };
    let ok_payload_last = ok_payload_last.unwrap();
    let ok_payload_end = ok_payload_last + 1;

    // The discriminant lives in bytes that differ between Ok and Err and
    // are *outside* the payload range (so they were equal across the two
    // Ok samples). Build a match/store pattern out of every such byte.
    let mut ok_match_entries = Vec::new();
    let mut err_match_entries = Vec::new();
    for i in 0..size {
        if i >= ok_payload_first && i < ok_payload_end {
            continue;
        }
        if ok_zero_bytes[i] != err_zero_bytes[i] {
            ok_match_entries.push(BytePattern::full(i as u32, ok_zero_bytes[i]));
            err_match_entries.push(BytePattern::full(i as u32, err_zero_bytes[i]));
        }
    }
    if ok_match_entries.is_empty() {
        return Err("probe failed: no discriminant bytes found between Ok and Err".to_string());
    }

    // store_pattern for explicit-tag enums is the same as match_pattern.
    let ok_store_entries = ok_match_entries.clone();
    let err_store_entries = err_match_entries.clone();

    let t_layout_ptr = arena.alloc_layout(t_layout);
    let e_layout_ptr = arena.alloc_layout(e_layout);

    let ok_field = FieldLayout {
        name: arena.alloc_str("0"),
        offset: ok_payload_first as u32,
        _pad: 0,
        layout: t_layout_ptr,
    };
    let (ok_fields_ptr, ok_field_count) = arena.alloc_fields(vec![ok_field]);

    let (err_fields_ptr, err_field_count) = if size_of::<E>() == 0 {
        (std::ptr::null(), 0)
    } else {
        let err_field = FieldLayout {
            name: arena.alloc_str("0"),
            offset: 0,
            _pad: 0,
            layout: e_layout_ptr,
        };
        arena.alloc_fields(vec![err_field])
    };

    let (ok_match_ptr, ok_match_count) = arena.alloc_patterns(ok_match_entries);
    let (ok_store_ptr, ok_store_count) = arena.alloc_patterns(ok_store_entries);
    let (err_match_ptr, err_match_count) = arena.alloc_patterns(err_match_entries);
    let (err_store_ptr, err_store_count) = arena.alloc_patterns(err_store_entries);

    let variants = vec![
        VariantLayout {
            name: arena.alloc_str("Ok"),
            match_pattern: ok_match_ptr,
            match_pattern_count: ok_match_count,
            store_pattern: ok_store_ptr,
            store_pattern_count: ok_store_count,
            fields: ok_fields_ptr,
            field_count: ok_field_count,
            _pad: 0,
        },
        VariantLayout {
            name: arena.alloc_str("Err"),
            match_pattern: err_match_ptr,
            match_pattern_count: err_match_count,
            store_pattern: err_store_ptr,
            store_pattern_count: err_store_count,
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
        variants: variants_ptr,
        variant_count,
        opaque_handle: 0,
        _reserved: 0,
    })
}

// ---------------------------------------------------------------------------
// Probe: niche-filled Option<T> by byte-comparing samples
// ---------------------------------------------------------------------------

/// Probe a niche-filled `Option<T>`'s in-memory layout into the given
/// arena.
///
/// Designed for the case where rustc has elided the discriminant by
/// reusing an unused bit-pattern in the payload (e.g.
/// `Option<Box<T>>`, `Option<&T>`, `Option<NonZeroU32>`, …): there is
/// no separate tag region — `None` is encoded as a specific pattern
/// across the payload bytes, and any other bit-pattern is `Some(_)`.
///
/// Caller supplies the pre-built byte representations of three sample
/// values (a `None` and two distinct `Some(_)`s), the value's `size` /
/// `align`, and the layout of the inner `T`. Taking bytes (rather than
/// `T`) keeps the probe usable with non-`Copy` payload types like
/// `Box<T>` without forcing the caller to leak or arena-allocate the
/// samples.
///
/// # Safety
/// `none_bytes`, `some_a_bytes`, and `some_b_bytes` must each point to
/// a readable buffer of exactly `size` bytes representing a valid
/// in-memory `Option<T>` value.
pub unsafe fn probe_option_niche_layout(
    arena: &LayoutArena,
    size: usize,
    align: usize,
    none_bytes_ptr: *const u8,
    some_a_bytes_ptr: *const u8,
    some_b_bytes_ptr: *const u8,
    t_layout: ValueLayout,
) -> Result<ValueLayout, String> {
    let none_bytes = unsafe { std::slice::from_raw_parts(none_bytes_ptr, size) };
    let a_bytes = unsafe { std::slice::from_raw_parts(some_a_bytes_ptr, size) };
    let b_bytes = unsafe { std::slice::from_raw_parts(some_b_bytes_ptr, size) };

    // For a niche-filled `Option<T>` there is no separate tag region —
    // the entire value is the payload. The match pattern for `None` is
    // therefore "every byte of the value equals the corresponding byte
    // of the canonical `None` representation." We don't try to infer a
    // narrower payload range from sample diffs, because two `Some`
    // samples may coincidentally share bytes (e.g. heap pointers in the
    // same address range share their high bits).
    let mut none_pattern: Vec<BytePattern> = Vec::new();
    for i in 0..size {
        none_pattern.push(BytePattern::full(i as u32, none_bytes[i]));
    }

    // Sanity: the None bytes must NOT match either Some sample byte-for-
    // byte, otherwise our pattern would mismatch a real Some as None.
    let none_matches_a = (0..size).all(|i| none_bytes[i] == a_bytes[i]);
    let none_matches_b = (0..size).all(|i| none_bytes[i] == b_bytes[i]);
    if none_matches_a || none_matches_b {
        return Err(
            "probe failed: None's bit-pattern collides with Some's — niche calibration unsound"
                .into(),
        );
    }

    let t_layout_ptr = arena.alloc_layout(t_layout);
    let some_field = FieldLayout {
        name: arena.alloc_str("0"),
        offset: 0,
        _pad: 0,
        layout: t_layout_ptr,
    };
    let (some_fields_ptr, some_field_count) = arena.alloc_fields(vec![some_field]);

    // Order: None first (its match_pattern catches the niche bit-pattern);
    // Some last (default / catch-all, empty match_pattern).
    let (none_match_ptr, none_match_count) = arena.alloc_patterns(none_pattern.clone());
    let (none_store_ptr, none_store_count) = arena.alloc_patterns(none_pattern);

    let variants = vec![
        VariantLayout {
            name: arena.alloc_str("None"),
            match_pattern: none_match_ptr,
            match_pattern_count: none_match_count,
            store_pattern: none_store_ptr,
            store_pattern_count: none_store_count,
            fields: std::ptr::null(),
            field_count: 0,
            _pad: 0,
        },
        VariantLayout {
            name: arena.alloc_str("Some"),
            match_pattern: std::ptr::null(),
            match_pattern_count: 0,
            store_pattern: std::ptr::null(),
            store_pattern_count: 0,
            fields: some_fields_ptr,
            field_count: some_field_count,
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

        let variants = layout.variants_slice();
        let ok_variant = variants
            .iter()
            .find(|v| v.name.as_str() == Some("Ok"))
            .unwrap();
        assert!(!ok_variant.is_default());
        assert!(!ok_variant.match_pattern_slice().is_empty());

        let mut storage: MaybeUninit<Result<u64, ()>> = MaybeUninit::uninit();
        let dst = storage.as_mut_ptr() as *mut u8;
        unsafe {
            std::ptr::write_bytes(dst, 0, layout.size as usize);

            // Apply Ok's store_pattern (writes the discriminant) and then
            // store the u64 payload at the field offset. Two operations,
            // both expressed as bytes-at-offsets — no helper.
            apply_store_pattern(ok_variant.store_pattern_slice(), dst);
            let payload_offset = ok_variant.fields_slice()[0].offset as usize;
            dst.add(payload_offset)
                .cast::<u64>()
                .write_unaligned(31_u64);
        }

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
        unsafe {
            std::ptr::write_bytes(dst, 0, layout.size as usize);
            let variants = layout.variants_slice();
            let err = variants
                .iter()
                .find(|v| v.name.as_str() == Some("Err"))
                .unwrap();
            apply_store_pattern(err.store_pattern_slice(), dst);
            // Err(()) has no payload to store.
        }

        let result: Result<u64, ()> = unsafe { storage.assume_init() };
        assert_eq!(result, Err(()));
    }

    #[test]
    fn match_pattern_recognises_constructed_variant() {
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

        let value: Result<u64, ()> = Ok(42);
        let bytes = &value as *const _ as *const u8;
        let variants = layout.variants_slice();
        let ok = variants
            .iter()
            .find(|v| v.name.as_str() == Some("Ok"))
            .unwrap();
        let err = variants
            .iter()
            .find(|v| v.name.as_str() == Some("Err"))
            .unwrap();
        unsafe {
            assert!(matches_pattern(ok.match_pattern_slice(), bytes));
            assert!(!matches_pattern(err.match_pattern_slice(), bytes));
        }
    }

    #[test]
    fn primitive_layout_constructs_correctly() {
        let layout = ValueLayout::primitive(PrimitiveKind::U32);
        assert_eq!(layout.kind, ValueLayoutKind::Primitive);
        assert_eq!(layout.size, 4);
        assert_eq!(layout.align, 4);
        assert_eq!(layout.primitive_kind, PrimitiveKind::U32);
    }

    /// Niche-filled `Option<Box<u64>>`: there is no separate
    /// discriminant — `None` is "all 8 bytes zero," `Some(_)` is
    /// "anything else." The pattern model handles this by giving
    /// `None` a match/store pattern of 8 zero bytes and `Some` an empty
    /// (default) match pattern.
    #[test]
    fn niche_probe_option_box_u64() {
        let arena = LayoutArena::new();
        let size = std::mem::size_of::<Option<Box<u64>>>();
        let align = std::mem::align_of::<Option<Box<u64>>>();

        // Build canonical None and two Some samples and snapshot their
        // bytes into stable buffers so we can drop the originals.
        let none: Option<Box<u64>> = None;
        let some_a: Option<Box<u64>> = Some(Box::new(0x1111_1111_1111_1111));
        let some_b: Option<Box<u64>> = Some(Box::new(0x2222_2222_2222_2222));
        let mut none_buf = vec![0u8; size];
        let mut a_buf = vec![0u8; size];
        let mut b_buf = vec![0u8; size];
        unsafe {
            std::ptr::copy_nonoverlapping(
                &none as *const _ as *const u8,
                none_buf.as_mut_ptr(),
                size,
            );
            std::ptr::copy_nonoverlapping(
                &some_a as *const _ as *const u8,
                a_buf.as_mut_ptr(),
                size,
            );
            std::ptr::copy_nonoverlapping(
                &some_b as *const _ as *const u8,
                b_buf.as_mut_ptr(),
                size,
            );
        }
        // Originals stay alive through the probe (the byte buffers
        // contain raw heap addresses, but we don't deref them, just
        // compare byte values).

        let layout = unsafe {
            probe_option_niche_layout(
                &arena,
                size,
                align,
                none_buf.as_ptr(),
                a_buf.as_ptr(),
                b_buf.as_ptr(),
                ValueLayout::empty_opaque(),
            )
        }
        .expect("probe Option<Box<u64>>");

        drop(some_a);
        drop(some_b);
        drop(none);

        assert_eq!(layout.kind, ValueLayoutKind::Enum);
        assert_eq!(layout.variant_count, 2);
        assert_eq!(layout.size as usize, size);

        let variants = layout.variants_slice();
        let none_variant = variants
            .iter()
            .find(|v| v.name.as_str() == Some("None"))
            .unwrap();
        let some_variant = variants
            .iter()
            .find(|v| v.name.as_str() == Some("Some"))
            .unwrap();

        // None has a per-byte zero pattern across the whole 8-byte payload.
        let none_pattern = none_variant.match_pattern_slice();
        assert_eq!(none_pattern.len(), 8);
        for entry in none_pattern {
            assert_eq!(entry.value, 0);
            assert_eq!(entry.mask, 0xFF);
        }
        // Some is the default catch-all.
        assert!(some_variant.is_default());
        assert!(some_variant.store_pattern_slice().is_empty());

        // Layout-driven match: a real None value's bytes match the None
        // pattern; a real Some value's bytes don't.
        let real_none: Option<Box<u64>> = None;
        let real_some: Option<Box<u64>> = Some(Box::new(99));
        unsafe {
            assert!(matches_pattern(
                none_variant.match_pattern_slice(),
                &real_none as *const _ as *const u8,
            ));
            assert!(!matches_pattern(
                none_variant.match_pattern_slice(),
                &real_some as *const _ as *const u8,
            ));
            // Some matches anything (default).
            assert!(matches_pattern(
                some_variant.match_pattern_slice(),
                &real_some as *const _ as *const u8,
            ));
        }

        // Layout-driven construct: store None's pattern into a fresh
        // buffer, then assume_init — the resulting Option must be None
        // and Drop must safely release nothing.
        let mut storage: MaybeUninit<Option<Box<u64>>> = MaybeUninit::uninit();
        let dst = storage.as_mut_ptr() as *mut u8;
        unsafe {
            apply_store_pattern(none_variant.store_pattern_slice(), dst);
        }
        let result: Option<Box<u64>> = unsafe { storage.assume_init() };
        assert!(result.is_none());
    }
}
