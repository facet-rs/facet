/// Write a CBOR unsigned integer (major type 0).
pub fn write_uint(out: &mut Vec<u8>, value: u64) {
    write_major(out, 0, value);
}

/// Write a CBOR negative integer (major type 1).
/// Encodes the value -1 - n, so to encode -1 pass n=0, to encode -10 pass n=9.
pub fn write_neg(out: &mut Vec<u8>, n: u64) {
    write_major(out, 1, n);
}

/// Write a CBOR byte string (major type 2).
pub fn write_bytes(out: &mut Vec<u8>, data: &[u8]) {
    write_major(out, 2, data.len() as u64);
    out.extend_from_slice(data);
}

/// Write a CBOR text string (major type 3).
pub fn write_text(out: &mut Vec<u8>, s: &str) {
    write_major(out, 3, s.len() as u64);
    out.extend_from_slice(s.as_bytes());
}

/// Write a CBOR array header (major type 4).
pub fn write_array_header(out: &mut Vec<u8>, len: u64) {
    write_major(out, 4, len);
}

/// Write a CBOR map header (major type 5).
pub fn write_map_header(out: &mut Vec<u8>, len: u64) {
    write_major(out, 5, len);
}

/// Write CBOR null (0xf6).
pub fn write_null(out: &mut Vec<u8>) {
    out.push(0xf6);
}

/// Write CBOR boolean.
pub fn write_bool(out: &mut Vec<u8>, value: bool) {
    out.push(if value { 0xf5 } else { 0xf4 });
}

/// Write CBOR float32 (0xfa + 4 bytes big-endian).
pub fn write_f32(out: &mut Vec<u8>, value: f32) {
    out.push(0xfa);
    out.extend_from_slice(&value.to_be_bytes());
}

/// Write CBOR float64 (0xfb + 8 bytes big-endian).
pub fn write_f64(out: &mut Vec<u8>, value: f64) {
    out.push(0xfb);
    out.extend_from_slice(&value.to_be_bytes());
}

/// Write a major type with minimal encoding (RFC 8949 §4.2).
fn write_major(out: &mut Vec<u8>, major: u8, value: u64) {
    let major_bits = major << 5;
    match value {
        0..=23 => {
            out.push(major_bits | value as u8);
        }
        24..=0xff => {
            out.push(major_bits | 24);
            out.push(value as u8);
        }
        0x100..=0xffff => {
            out.push(major_bits | 25);
            out.extend_from_slice(&(value as u16).to_be_bytes());
        }
        0x10000..=0xffff_ffff => {
            out.push(major_bits | 26);
            out.extend_from_slice(&(value as u32).to_be_bytes());
        }
        _ => {
            out.push(major_bits | 27);
            out.extend_from_slice(&value.to_be_bytes());
        }
    }
}
