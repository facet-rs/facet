use crate::error::CborError;

/// Read the initial byte of a CBOR item, returning (major_type, additional_info).
/// Does NOT consume argument bytes — use `read_argument` for that.
fn read_initial_byte(input: &[u8], offset: &mut usize) -> Result<(u8, u8), CborError> {
    if *offset >= input.len() {
        return Err(CborError::UnexpectedEof);
    }
    let byte = input[*offset];
    *offset += 1;
    Ok((byte >> 5, byte & 0x1f))
}

/// Read the argument value for a given additional_info.
/// Returns the u64 argument. For additional_info 0..=23, returns the value directly.
fn read_argument(input: &[u8], offset: &mut usize, additional: u8) -> Result<u64, CborError> {
    match additional {
        0..=23 => Ok(additional as u64),
        24 => {
            if *offset >= input.len() {
                return Err(CborError::UnexpectedEof);
            }
            let v = input[*offset];
            *offset += 1;
            Ok(v as u64)
        }
        25 => {
            if *offset + 2 > input.len() {
                return Err(CborError::UnexpectedEof);
            }
            let v = u16::from_be_bytes([input[*offset], input[*offset + 1]]);
            *offset += 2;
            Ok(v as u64)
        }
        26 => {
            if *offset + 4 > input.len() {
                return Err(CborError::UnexpectedEof);
            }
            let v = u32::from_be_bytes([
                input[*offset],
                input[*offset + 1],
                input[*offset + 2],
                input[*offset + 3],
            ]);
            *offset += 4;
            Ok(v as u64)
        }
        27 => {
            if *offset + 8 > input.len() {
                return Err(CborError::UnexpectedEof);
            }
            let v = u64::from_be_bytes([
                input[*offset],
                input[*offset + 1],
                input[*offset + 2],
                input[*offset + 3],
                input[*offset + 4],
                input[*offset + 5],
                input[*offset + 6],
                input[*offset + 7],
            ]);
            *offset += 8;
            Ok(v)
        }
        _ => Err(CborError::InvalidCbor(format!(
            "unsupported additional info: {additional}"
        ))),
    }
}

/// Read a CBOR unsigned integer (major type 0). Returns the u64 value.
pub fn read_uint(input: &[u8], offset: &mut usize) -> Result<u64, CborError> {
    let (major, additional) = read_initial_byte(input, offset)?;
    if major != 0 {
        return Err(CborError::TypeMismatch {
            expected: "unsigned integer (major 0)".into(),
            got: format!("major type {major}"),
        });
    }
    read_argument(input, offset, additional)
}

/// Read a CBOR negative integer (major type 1). Returns the n in -1-n.
pub fn read_neg(input: &[u8], offset: &mut usize) -> Result<u64, CborError> {
    let (major, additional) = read_initial_byte(input, offset)?;
    if major != 1 {
        return Err(CborError::TypeMismatch {
            expected: "negative integer (major 1)".into(),
            got: format!("major type {major}"),
        });
    }
    read_argument(input, offset, additional)
}

/// Read a CBOR byte string (major type 2). Returns the byte slice.
pub fn read_bytes<'a>(input: &'a [u8], offset: &mut usize) -> Result<&'a [u8], CborError> {
    let (major, additional) = read_initial_byte(input, offset)?;
    if major != 2 {
        return Err(CborError::TypeMismatch {
            expected: "byte string (major 2)".into(),
            got: format!("major type {major}"),
        });
    }
    let len = read_argument(input, offset, additional)? as usize;
    if *offset + len > input.len() {
        return Err(CborError::UnexpectedEof);
    }
    let data = &input[*offset..*offset + len];
    *offset += len;
    Ok(data)
}

/// Read a CBOR text string (major type 3). Returns the string slice.
pub fn read_text<'a>(input: &'a [u8], offset: &mut usize) -> Result<&'a str, CborError> {
    let (major, additional) = read_initial_byte(input, offset)?;
    if major != 3 {
        return Err(CborError::TypeMismatch {
            expected: "text string (major 3)".into(),
            got: format!("major type {major}"),
        });
    }
    let len = read_argument(input, offset, additional)? as usize;
    if *offset + len > input.len() {
        return Err(CborError::UnexpectedEof);
    }
    let data = &input[*offset..*offset + len];
    *offset += len;
    core::str::from_utf8(data)
        .map_err(|e| CborError::InvalidCbor(format!("invalid UTF-8 in text string: {e}")))
}

/// Read a CBOR array header (major type 4). Returns the length.
pub fn read_array_header(input: &[u8], offset: &mut usize) -> Result<u64, CborError> {
    let (major, additional) = read_initial_byte(input, offset)?;
    if major != 4 {
        return Err(CborError::TypeMismatch {
            expected: "array (major 4)".into(),
            got: format!("major type {major}"),
        });
    }
    read_argument(input, offset, additional)
}

/// Read a CBOR map header (major type 5). Returns the number of key-value pairs.
pub fn read_map_header(input: &[u8], offset: &mut usize) -> Result<u64, CborError> {
    let (major, additional) = read_initial_byte(input, offset)?;
    if major != 5 {
        return Err(CborError::TypeMismatch {
            expected: "map (major 5)".into(),
            got: format!("major type {major}"),
        });
    }
    read_argument(input, offset, additional)
}

/// Read a CBOR boolean (simple values 20=false, 21=true).
pub fn read_bool(input: &[u8], offset: &mut usize) -> Result<bool, CborError> {
    if *offset >= input.len() {
        return Err(CborError::UnexpectedEof);
    }
    let byte = input[*offset];
    match byte {
        0xf4 => {
            *offset += 1;
            Ok(false)
        }
        0xf5 => {
            *offset += 1;
            Ok(true)
        }
        _ => Err(CborError::TypeMismatch {
            expected: "boolean (0xf4 or 0xf5)".into(),
            got: format!("byte 0x{byte:02x}"),
        }),
    }
}

/// Read a CBOR float32 (0xfa + 4 bytes big-endian).
pub fn read_f32(input: &[u8], offset: &mut usize) -> Result<f32, CborError> {
    if *offset >= input.len() {
        return Err(CborError::UnexpectedEof);
    }
    if input[*offset] != 0xfa {
        return Err(CborError::TypeMismatch {
            expected: "float32 (0xfa)".into(),
            got: format!("byte 0x{:02x}", input[*offset]),
        });
    }
    *offset += 1;
    if *offset + 4 > input.len() {
        return Err(CborError::UnexpectedEof);
    }
    let v = f32::from_be_bytes([
        input[*offset],
        input[*offset + 1],
        input[*offset + 2],
        input[*offset + 3],
    ]);
    *offset += 4;
    Ok(v)
}

/// Read a CBOR float64 (0xfb + 8 bytes big-endian).
pub fn read_f64(input: &[u8], offset: &mut usize) -> Result<f64, CborError> {
    if *offset >= input.len() {
        return Err(CborError::UnexpectedEof);
    }
    if input[*offset] != 0xfb {
        return Err(CborError::TypeMismatch {
            expected: "float64 (0xfb)".into(),
            got: format!("byte 0x{:02x}", input[*offset]),
        });
    }
    *offset += 1;
    if *offset + 8 > input.len() {
        return Err(CborError::UnexpectedEof);
    }
    let v = f64::from_be_bytes([
        input[*offset],
        input[*offset + 1],
        input[*offset + 2],
        input[*offset + 3],
        input[*offset + 4],
        input[*offset + 5],
        input[*offset + 6],
        input[*offset + 7],
    ]);
    *offset += 8;
    Ok(v)
}

/// Check if the next CBOR value is null (0xf6) without consuming it.
pub fn is_null(input: &[u8], offset: usize) -> bool {
    offset < input.len() && input[offset] == 0xf6
}

/// Consume a null value (0xf6). Advances offset by 1.
pub fn read_null(input: &[u8], offset: &mut usize) -> Result<(), CborError> {
    if *offset >= input.len() {
        return Err(CborError::UnexpectedEof);
    }
    if input[*offset] != 0xf6 {
        return Err(CborError::TypeMismatch {
            expected: "null (0xf6)".into(),
            got: format!("byte 0x{:02x}", input[*offset]),
        });
    }
    *offset += 1;
    Ok(())
}

/// Skip one complete CBOR value, advancing the offset past it.
pub fn skip_value(input: &[u8], offset: &mut usize) -> Result<(), CborError> {
    if *offset >= input.len() {
        return Err(CborError::UnexpectedEof);
    }
    let byte = input[*offset];
    let major = byte >> 5;
    let additional = byte & 0x1f;
    *offset += 1;

    match major {
        0 | 1 => {
            // unsigned or negative integer — just consume the argument bytes
            let _ = read_argument(input, offset, additional)?;
        }
        2 | 3 => {
            // byte string or text string — consume argument + payload
            let len = read_argument(input, offset, additional)? as usize;
            if *offset + len > input.len() {
                return Err(CborError::UnexpectedEof);
            }
            *offset += len;
        }
        4 => {
            // array — skip N items
            let len = read_argument(input, offset, additional)?;
            for _ in 0..len {
                skip_value(input, offset)?;
            }
        }
        5 => {
            // map — skip N key-value pairs
            let len = read_argument(input, offset, additional)?;
            for _ in 0..len {
                skip_value(input, offset)?;
                skip_value(input, offset)?;
            }
        }
        6 => {
            // tag — consume tag number then skip the tagged value
            let _ = read_argument(input, offset, additional)?;
            skip_value(input, offset)?;
        }
        7 => {
            // simple values and floats
            match additional {
                0..=23 => { /* simple value, already consumed */ }
                24 => {
                    if *offset >= input.len() {
                        return Err(CborError::UnexpectedEof);
                    }
                    *offset += 1;
                }
                25 => {
                    if *offset + 2 > input.len() {
                        return Err(CborError::UnexpectedEof);
                    }
                    *offset += 2;
                }
                26 => {
                    if *offset + 4 > input.len() {
                        return Err(CborError::UnexpectedEof);
                    }
                    *offset += 4;
                }
                27 => {
                    if *offset + 8 > input.len() {
                        return Err(CborError::UnexpectedEof);
                    }
                    *offset += 8;
                }
                _ => {
                    return Err(CborError::InvalidCbor(format!(
                        "unsupported simple value additional info: {additional}"
                    )));
                }
            }
        }
        _ => {
            return Err(CborError::InvalidCbor(format!(
                "unknown major type: {major}"
            )));
        }
    }
    Ok(())
}

/// Peek at the major type of the next CBOR value without consuming it.
pub fn peek_major(input: &[u8], offset: usize) -> Result<u8, CborError> {
    if offset >= input.len() {
        return Err(CborError::UnexpectedEof);
    }
    Ok(input[offset] >> 5)
}

/// Read either an unsigned (major 0) or negative (major 1) integer as i64.
pub fn read_int_as_i64(input: &[u8], offset: &mut usize) -> Result<i64, CborError> {
    let major = peek_major(input, *offset)?;
    match major {
        0 => {
            let v = read_uint(input, offset)?;
            i64::try_from(v)
                .map_err(|_| CborError::InvalidCbor(format!("unsigned integer {v} overflows i64")))
        }
        1 => {
            let n = read_neg(input, offset)?;
            // value = -1 - n
            if n > i64::MAX as u64 {
                return Err(CborError::InvalidCbor(format!(
                    "negative integer -1-{n} overflows i64"
                )));
            }
            Ok(-1i64 - n as i64)
        }
        _ => Err(CborError::TypeMismatch {
            expected: "integer (major 0 or 1)".into(),
            got: format!("major type {major}"),
        }),
    }
}

/// Read either an unsigned (major 0) or negative (major 1) integer as u64.
/// Negative integers are rejected.
pub fn read_int_as_u64(input: &[u8], offset: &mut usize) -> Result<u64, CborError> {
    let major = peek_major(input, *offset)?;
    match major {
        0 => read_uint(input, offset),
        1 => Err(CborError::TypeMismatch {
            expected: "unsigned integer".into(),
            got: "negative integer".into(),
        }),
        _ => Err(CborError::TypeMismatch {
            expected: "integer (major 0)".into(),
            got: format!("major type {major}"),
        }),
    }
}
