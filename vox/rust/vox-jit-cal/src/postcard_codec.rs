//! Postcard encode / decode driven by a [`ValueLayout`].
//!
//! Reference data-driven implementation: walks the layout and emits or
//! consumes postcard bytes directly. The codec the JIT eventually
//! generates produces *exactly* this byte sequence, just with the layout
//! constants baked into machine code instead of read from memory at
//! every step.
//!
//! Coverage right now: primitives (bool, u8..u64, i8..i64, f32, f64,
//! unit), structs (each field in order), and enums (varint variant
//! index + the matching variant's fields). String / bytes / containers /
//! recursion are deferred until the calibration side learns those.

use crate::value_layout::{
    BytePattern, FieldLayout, PrimitiveKind, ValueLayout, ValueLayoutKind, VariantLayout,
    apply_store_pattern, matches_pattern,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecError {
    /// Tried to operate on a layout shape we haven't taught the codec to
    /// handle yet (e.g. an opaque container).
    Unsupported,
    /// Postcard input ended before the value was complete.
    UnexpectedEof,
    /// A varint exceeded the maximum encoded width.
    VarintOverflow,
    /// On encode, the value's bytes didn't match any of the layout's
    /// variant `match_pattern`s (and there was no default variant).
    NoMatchingVariant,
    /// The decoded variant index was out of range for the layout.
    InvalidVariantIndex,
    /// A bool byte was neither 0x00 nor 0x01.
    InvalidBool,
}

// ---------------------------------------------------------------------------
// Varint helpers (LEB128, postcard-style)
// ---------------------------------------------------------------------------

fn write_varint_u64(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
            out.push(byte);
        } else {
            out.push(byte);
            return;
        }
    }
}

fn read_varint_u64(input: &[u8]) -> Result<(u64, usize), CodecError> {
    let mut value: u64 = 0;
    let mut shift: u32 = 0;
    let mut consumed = 0;
    loop {
        let byte = *input.get(consumed).ok_or(CodecError::UnexpectedEof)?;
        consumed += 1;
        value |= ((byte & 0x7F) as u64) << shift;
        if byte & 0x80 == 0 {
            return Ok((value, consumed));
        }
        shift += 7;
        if shift >= 64 {
            return Err(CodecError::VarintOverflow);
        }
    }
}

fn write_zigzag_i64(out: &mut Vec<u8>, value: i64) {
    let zz = ((value << 1) ^ (value >> 63)) as u64;
    write_varint_u64(out, zz);
}

fn read_zigzag_i64(input: &[u8]) -> Result<(i64, usize), CodecError> {
    let (zz, n) = read_varint_u64(input)?;
    let v = ((zz >> 1) as i64) ^ (-((zz & 1) as i64));
    Ok((v, n))
}

// ---------------------------------------------------------------------------
// Encode
// ---------------------------------------------------------------------------

/// Encode the value at `value_ptr` (which must match `layout`) into postcard
/// bytes appended to `out`.
///
/// # Safety
/// `value_ptr` must point to a valid value of the type described by
/// `layout` (i.e. at least `layout.size` readable bytes containing a
/// well-formed instance).
pub unsafe fn encode_value(
    layout: &ValueLayout,
    value_ptr: *const u8,
    out: &mut Vec<u8>,
) -> Result<(), CodecError> {
    match layout.kind {
        ValueLayoutKind::Primitive => unsafe {
            encode_primitive(layout.primitive_kind, value_ptr, out)
        },
        ValueLayoutKind::Struct => unsafe { encode_fields(layout.fields_slice(), value_ptr, out) },
        ValueLayoutKind::Enum => unsafe { encode_enum(layout, value_ptr, out) },
        ValueLayoutKind::Opaque => Err(CodecError::Unsupported),
    }
}

unsafe fn encode_primitive(
    kind: PrimitiveKind,
    value_ptr: *const u8,
    out: &mut Vec<u8>,
) -> Result<(), CodecError> {
    match kind {
        PrimitiveKind::Unit => Ok(()),
        PrimitiveKind::Bool => {
            let b = unsafe { value_ptr.read() };
            if b > 1 {
                return Err(CodecError::InvalidBool);
            }
            out.push(b);
            Ok(())
        }
        PrimitiveKind::U8 => {
            out.push(unsafe { value_ptr.read() });
            Ok(())
        }
        PrimitiveKind::I8 => {
            out.push(unsafe { value_ptr.read() });
            Ok(())
        }
        PrimitiveKind::U16 => {
            let v = unsafe { value_ptr.cast::<u16>().read_unaligned() } as u64;
            write_varint_u64(out, v);
            Ok(())
        }
        PrimitiveKind::U32 => {
            let v = unsafe { value_ptr.cast::<u32>().read_unaligned() } as u64;
            write_varint_u64(out, v);
            Ok(())
        }
        PrimitiveKind::U64 => {
            let v = unsafe { value_ptr.cast::<u64>().read_unaligned() };
            write_varint_u64(out, v);
            Ok(())
        }
        PrimitiveKind::I16 => {
            let v = unsafe { value_ptr.cast::<i16>().read_unaligned() } as i64;
            write_zigzag_i64(out, v);
            Ok(())
        }
        PrimitiveKind::I32 => {
            let v = unsafe { value_ptr.cast::<i32>().read_unaligned() } as i64;
            write_zigzag_i64(out, v);
            Ok(())
        }
        PrimitiveKind::I64 => {
            let v = unsafe { value_ptr.cast::<i64>().read_unaligned() };
            write_zigzag_i64(out, v);
            Ok(())
        }
        PrimitiveKind::F32 => {
            let v = unsafe { value_ptr.cast::<f32>().read_unaligned() };
            out.extend_from_slice(&v.to_le_bytes());
            Ok(())
        }
        PrimitiveKind::F64 => {
            let v = unsafe { value_ptr.cast::<f64>().read_unaligned() };
            out.extend_from_slice(&v.to_le_bytes());
            Ok(())
        }
    }
}

unsafe fn encode_fields(
    fields: &[FieldLayout],
    base_ptr: *const u8,
    out: &mut Vec<u8>,
) -> Result<(), CodecError> {
    for field in fields {
        let field_ptr = unsafe { base_ptr.add(field.offset as usize) };
        unsafe { encode_value(field.layout(), field_ptr, out)? };
    }
    Ok(())
}

unsafe fn encode_enum(
    layout: &ValueLayout,
    value_ptr: *const u8,
    out: &mut Vec<u8>,
) -> Result<(), CodecError> {
    let variants = layout.variants_slice();
    // Find the variant whose match_pattern accepts these bytes. Variants
    // with non-empty patterns are tested first; the default variant
    // (empty pattern) is the catch-all.
    let mut chosen: Option<(usize, &VariantLayout)> = None;
    for (idx, variant) in variants.iter().enumerate() {
        if variant.is_default() {
            continue;
        }
        if unsafe { matches_pattern(variant.match_pattern_slice(), value_ptr) } {
            chosen = Some((idx, variant));
            break;
        }
    }
    if chosen.is_none() {
        for (idx, variant) in variants.iter().enumerate() {
            if variant.is_default() {
                chosen = Some((idx, variant));
                break;
            }
        }
    }
    let (variant_index, variant) = chosen.ok_or(CodecError::NoMatchingVariant)?;

    write_varint_u64(out, variant_index as u64);
    unsafe { encode_fields(variant.fields_slice(), value_ptr, out) }
}

// ---------------------------------------------------------------------------
// Decode
// ---------------------------------------------------------------------------

/// Decode postcard bytes from `input` into `dst` (which must be writable
/// for `layout.size` bytes). Returns the number of input bytes consumed.
///
/// # Safety
/// `dst` must point to writable storage of at least `layout.size` bytes,
/// suitably aligned to `layout.align`.
pub unsafe fn decode_value(
    layout: &ValueLayout,
    input: &[u8],
    dst: *mut u8,
) -> Result<usize, CodecError> {
    match layout.kind {
        ValueLayoutKind::Primitive => unsafe {
            decode_primitive(layout.primitive_kind, input, dst)
        },
        ValueLayoutKind::Struct => unsafe { decode_fields(layout.fields_slice(), input, dst) },
        ValueLayoutKind::Enum => unsafe { decode_enum(layout, input, dst) },
        ValueLayoutKind::Opaque => Err(CodecError::Unsupported),
    }
}

unsafe fn decode_primitive(
    kind: PrimitiveKind,
    input: &[u8],
    dst: *mut u8,
) -> Result<usize, CodecError> {
    match kind {
        PrimitiveKind::Unit => Ok(0),
        PrimitiveKind::Bool => {
            let b = *input.first().ok_or(CodecError::UnexpectedEof)?;
            if b > 1 {
                return Err(CodecError::InvalidBool);
            }
            unsafe { dst.write(b) };
            Ok(1)
        }
        PrimitiveKind::U8 | PrimitiveKind::I8 => {
            let b = *input.first().ok_or(CodecError::UnexpectedEof)?;
            unsafe { dst.write(b) };
            Ok(1)
        }
        PrimitiveKind::U16 => {
            let (v, n) = read_varint_u64(input)?;
            unsafe { dst.cast::<u16>().write_unaligned(v as u16) };
            Ok(n)
        }
        PrimitiveKind::U32 => {
            let (v, n) = read_varint_u64(input)?;
            unsafe { dst.cast::<u32>().write_unaligned(v as u32) };
            Ok(n)
        }
        PrimitiveKind::U64 => {
            let (v, n) = read_varint_u64(input)?;
            unsafe { dst.cast::<u64>().write_unaligned(v) };
            Ok(n)
        }
        PrimitiveKind::I16 => {
            let (v, n) = read_zigzag_i64(input)?;
            unsafe { dst.cast::<i16>().write_unaligned(v as i16) };
            Ok(n)
        }
        PrimitiveKind::I32 => {
            let (v, n) = read_zigzag_i64(input)?;
            unsafe { dst.cast::<i32>().write_unaligned(v as i32) };
            Ok(n)
        }
        PrimitiveKind::I64 => {
            let (v, n) = read_zigzag_i64(input)?;
            unsafe { dst.cast::<i64>().write_unaligned(v) };
            Ok(n)
        }
        PrimitiveKind::F32 => {
            let bytes = input.get(0..4).ok_or(CodecError::UnexpectedEof)?;
            let v = f32::from_le_bytes(bytes.try_into().unwrap());
            unsafe { dst.cast::<f32>().write_unaligned(v) };
            Ok(4)
        }
        PrimitiveKind::F64 => {
            let bytes = input.get(0..8).ok_or(CodecError::UnexpectedEof)?;
            let v = f64::from_le_bytes(bytes.try_into().unwrap());
            unsafe { dst.cast::<f64>().write_unaligned(v) };
            Ok(8)
        }
    }
}

unsafe fn decode_fields(
    fields: &[FieldLayout],
    input: &[u8],
    base_dst: *mut u8,
) -> Result<usize, CodecError> {
    let mut consumed = 0;
    for field in fields {
        let field_dst = unsafe { base_dst.add(field.offset as usize) };
        let n = unsafe { decode_value(field.layout(), &input[consumed..], field_dst)? };
        consumed += n;
    }
    Ok(consumed)
}

unsafe fn decode_enum(
    layout: &ValueLayout,
    input: &[u8],
    dst: *mut u8,
) -> Result<usize, CodecError> {
    let (variant_index, mut consumed) = read_varint_u64(input)?;
    let variants = layout.variants_slice();
    let variant = variants
        .get(variant_index as usize)
        .ok_or(CodecError::InvalidVariantIndex)?;

    // Apply the variant's store_pattern (writes the in-memory
    // discriminant for tag-based enums; empty for niche-filled variants
    // where the field stores produce the variant).
    unsafe { apply_store_pattern(variant.store_pattern_slice(), dst) };

    // Decode each field at its calibrated absolute offset (variant
    // fields carry absolute offsets, including the discriminant region).
    for field in variant.fields_slice() {
        let field_dst = unsafe { dst.add(field.offset as usize) };
        let n = unsafe { decode_value(field.layout(), &input[consumed..], field_dst)? };
        consumed += n;
    }
    Ok(consumed)
}

#[allow(unused)]
fn _force_byte_pattern_link(_: BytePattern) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value_layout::{LayoutArena, probe_result_layout};
    use std::mem::MaybeUninit;

    /// Encode `Ok(31): Result<u64, ()>` into postcard bytes via the
    /// layout-driven codec, then decode them back into a fresh
    /// `Result<u64, ()>` using the same layout. Verify byte-for-byte
    /// against postcard's expected output (`0x00, 0x1F`) and value-wise
    /// against `Ok(31)`.
    #[test]
    fn round_trip_result_u64_ok_31() {
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

        let value: Result<u64, ()> = Ok(31);
        let mut buf = Vec::new();
        unsafe {
            encode_value(&layout, &value as *const _ as *const u8, &mut buf).unwrap();
        }
        // postcard: variant 0 = Ok (varint 0x00), then u64 31 (varint 0x1F).
        assert_eq!(buf, vec![0x00, 0x1F]);

        let mut storage: MaybeUninit<Result<u64, ()>> = MaybeUninit::uninit();
        let dst = storage.as_mut_ptr() as *mut u8;
        unsafe {
            std::ptr::write_bytes(dst, 0, layout.size as usize);
            let consumed = decode_value(&layout, &buf, dst).unwrap();
            assert_eq!(consumed, buf.len());
        }
        let decoded: Result<u64, ()> = unsafe { storage.assume_init() };
        assert_eq!(decoded, Ok(31));
    }

    #[test]
    fn round_trip_result_u64_err_unit() {
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

        let value: Result<u64, ()> = Err(());
        let mut buf = Vec::new();
        unsafe {
            encode_value(&layout, &value as *const _ as *const u8, &mut buf).unwrap();
        }
        // Variant 1 = Err, no payload. One byte: 0x01.
        assert_eq!(buf, vec![0x01]);

        let mut storage: MaybeUninit<Result<u64, ()>> = MaybeUninit::uninit();
        let dst = storage.as_mut_ptr() as *mut u8;
        unsafe {
            std::ptr::write_bytes(dst, 0, layout.size as usize);
            let consumed = decode_value(&layout, &buf, dst).unwrap();
            assert_eq!(consumed, buf.len());
        }
        let decoded: Result<u64, ()> = unsafe { storage.assume_init() };
        assert_eq!(decoded, Err(()));
    }

    #[test]
    fn round_trip_u64_primitive() {
        let layout = ValueLayout::primitive(PrimitiveKind::U64);
        let value: u64 = 0xDEAD_BEEF_CAFE_BABE;
        let mut buf = Vec::new();
        unsafe {
            encode_value(&layout, &value as *const _ as *const u8, &mut buf).unwrap();
        }

        let mut decoded: u64 = 0;
        unsafe {
            let consumed = decode_value(&layout, &buf, &mut decoded as *mut _ as *mut u8).unwrap();
            assert_eq!(consumed, buf.len());
        }
        assert_eq!(decoded, value);
    }
}
