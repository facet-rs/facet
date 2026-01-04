//! ASN.1 DER serializer implementing FormatSerializer.

extern crate alloc;

use alloc::{string::String, vec::Vec};

use facet_format::{FormatSerializer, ScalarValue, SerializeError};

// ASN.1 Universal Tags
const TAG_BOOLEAN: u8 = 0x01;
const TAG_INTEGER: u8 = 0x02;
const TAG_OCTET_STRING: u8 = 0x04;
const TAG_NULL: u8 = 0x05;
const TAG_REAL: u8 = 0x09;
const TAG_UTF8STRING: u8 = 0x0C;
const TAG_SEQUENCE: u8 = 0x10;

const CONSTRUCTED_BIT: u8 = 0x20;

// Real format special values
const REAL_INFINITY: u8 = 0b01000000;
const REAL_NEG_INFINITY: u8 = 0b01000001;
const REAL_NAN: u8 = 0b01000010;
const REAL_NEG_ZERO: u8 = 0b01000011;

const F64_MANTISSA_MASK: u64 = 0b1111111111111111111111111111111111111111111111111111;

/// ASN.1 serialization error.
#[derive(Debug)]
pub struct Asn1SerializeError {
    message: String,
}

impl core::fmt::Display for Asn1SerializeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.message)
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Asn1SerializeError {}

/// ASN.1 DER serializer.
pub struct Asn1Serializer {
    out: Vec<u8>,
    /// Stack of positions where container lengths need to be patched
    stack: Vec<ContainerState>,
}

#[derive(Debug)]
struct ContainerState {
    /// Position of the length placeholder
    len_pos: usize,
}

impl Asn1Serializer {
    /// Create a new ASN.1 DER serializer.
    pub fn new() -> Self {
        Self {
            out: Vec::new(),
            stack: Vec::new(),
        }
    }

    /// Consume the serializer and return the output bytes.
    pub fn finish(self) -> Vec<u8> {
        self.out
    }

    /// Write a TLV with the given tag and value bytes.
    fn write_tlv(&mut self, tag: u8, value: &[u8]) {
        self.out.push(tag);
        self.write_length(value.len());
        self.out.extend_from_slice(value);
    }

    /// Write a length in DER format.
    fn write_length(&mut self, len: usize) {
        if len < 128 {
            self.out.push(len as u8);
        } else {
            // Count how many bytes we need for the length
            let mut temp = len;
            let mut bytes_needed = 0;
            while temp > 0 {
                bytes_needed += 1;
                temp >>= 8;
            }
            self.out.push(0x80 | bytes_needed);
            let len_bytes = len.to_be_bytes();
            self.out
                .extend_from_slice(&len_bytes[8 - bytes_needed as usize..]);
        }
    }

    /// Write a boolean value.
    fn write_bool(&mut self, value: bool) {
        let byte = if value { 0xFF } else { 0x00 };
        self.write_tlv(TAG_BOOLEAN, &[byte]);
    }

    /// Write an integer value.
    fn write_i64(&mut self, value: i64) {
        let bytes = value.to_be_bytes();
        // Find the minimal representation
        let mut leading_redundant = 0;
        for window in bytes.windows(2) {
            let byte = window[0] as i8;
            let bit = window[1] as i8 >> 7;
            if byte ^ bit == 0 {
                leading_redundant += 1;
            } else {
                break;
            }
        }
        self.write_tlv(TAG_INTEGER, &bytes[leading_redundant..]);
    }

    /// Write an unsigned integer value.
    fn write_u64(&mut self, value: u64) {
        let bytes = value.to_be_bytes();
        // Find leading zeros, but ensure we don't remove the sign bit
        let mut start = 0;
        while start < 7 && bytes[start] == 0 && (bytes[start + 1] & 0x80) == 0 {
            start += 1;
        }
        // If high bit is set, need to add a leading zero to keep it positive
        if bytes[start] & 0x80 != 0 {
            self.out.push(TAG_INTEGER);
            self.write_length(bytes.len() - start + 1);
            self.out.push(0x00);
            self.out.extend_from_slice(&bytes[start..]);
        } else {
            self.write_tlv(TAG_INTEGER, &bytes[start..]);
        }
    }

    /// Write a real (f64) value.
    fn write_f64(&mut self, value: f64) {
        use core::num::FpCategory;
        match value.classify() {
            FpCategory::Nan => self.write_tlv(TAG_REAL, &[REAL_NAN]),
            FpCategory::Infinite => {
                if value.is_sign_positive() {
                    self.write_tlv(TAG_REAL, &[REAL_INFINITY]);
                } else {
                    self.write_tlv(TAG_REAL, &[REAL_NEG_INFINITY]);
                }
            }
            FpCategory::Zero | FpCategory::Subnormal => {
                // Subnormals are rounded to zero in DER
                if value.is_sign_positive() {
                    self.write_tlv(TAG_REAL, &[]); // Positive zero is empty content
                } else {
                    self.write_tlv(TAG_REAL, &[REAL_NEG_ZERO]);
                }
            }
            FpCategory::Normal => {
                let sign_negative = value.is_sign_negative();
                let bits = value.to_bits();
                let mut exponent = ((bits >> 52) & 0b11111111111) as i16 - 1023;
                let mut mantissa = bits & F64_MANTISSA_MASK | (0b1 << 52);
                let mut normalization_factor = 52;
                while mantissa & 0b1 == 0 {
                    mantissa >>= 1;
                    normalization_factor -= 1;
                }
                exponent -= normalization_factor;

                let mantissa_bytes = mantissa.to_be_bytes();
                let mut leading_zero_bytes = 0;
                for byte in mantissa_bytes {
                    if byte == 0 {
                        leading_zero_bytes += 1;
                    } else {
                        break;
                    }
                }

                let exponent_bytes = exponent.to_be_bytes();
                let short_exp = exponent_bytes[0] == 0 || exponent_bytes[0] == 0xFF;
                let content_len =
                    2 + (!short_exp as usize) + mantissa_bytes.len() - leading_zero_bytes;

                let structure_byte = 0b10000000 | ((sign_negative as u8) << 6) | (!short_exp as u8);

                self.out.push(TAG_REAL);
                self.write_length(content_len);
                self.out.push(structure_byte);

                if short_exp {
                    self.out.push(exponent_bytes[1]);
                } else {
                    self.out.extend_from_slice(&exponent_bytes);
                }
                self.out
                    .extend_from_slice(&mantissa_bytes[leading_zero_bytes..]);
            }
        }
    }

    /// Write a UTF-8 string.
    fn write_str(&mut self, s: &str) {
        self.write_tlv(TAG_UTF8STRING, s.as_bytes());
    }

    /// Write binary data as OCTET STRING.
    fn write_bytes(&mut self, bytes: &[u8]) {
        self.write_tlv(TAG_OCTET_STRING, bytes);
    }

    /// Write a NULL value.
    fn write_null(&mut self) {
        self.write_tlv(TAG_NULL, &[]);
    }

    /// Begin a SEQUENCE (for struct).
    fn begin_sequence(&mut self) {
        self.out.push(TAG_SEQUENCE | CONSTRUCTED_BIT);
        let len_pos = self.out.len();
        // Placeholder for length - we'll use long form to avoid resizing
        self.out.extend_from_slice(&[0x84, 0, 0, 0, 0]); // Long form with 4 bytes
        self.stack.push(ContainerState { len_pos });
    }

    /// End a SEQUENCE and patch the length.
    fn end_sequence(&mut self) {
        if let Some(state) = self.stack.pop() {
            let content_len = self.out.len() - state.len_pos - 5; // Subtract the 5-byte placeholder

            // Patch the length (we used 4-byte long form)
            let len_bytes = (content_len as u32).to_be_bytes();
            self.out[state.len_pos] = 0x84; // Long form, 4 bytes
            self.out[state.len_pos + 1..state.len_pos + 5].copy_from_slice(&len_bytes);
        }
    }
}

impl Default for Asn1Serializer {
    fn default() -> Self {
        Self::new()
    }
}

impl FormatSerializer for Asn1Serializer {
    type Error = Asn1SerializeError;

    fn begin_struct(&mut self) -> Result<(), Self::Error> {
        self.begin_sequence();
        Ok(())
    }

    fn field_key(&mut self, _key: &str) -> Result<(), Self::Error> {
        // ASN.1 DER doesn't encode field names - fields are positional
        Ok(())
    }

    fn end_struct(&mut self) -> Result<(), Self::Error> {
        self.end_sequence();
        Ok(())
    }

    fn begin_seq(&mut self) -> Result<(), Self::Error> {
        self.begin_sequence();
        Ok(())
    }

    fn end_seq(&mut self) -> Result<(), Self::Error> {
        self.end_sequence();
        Ok(())
    }

    fn scalar(&mut self, scalar: ScalarValue<'_>) -> Result<(), Self::Error> {
        match scalar {
            ScalarValue::Null => self.write_null(),
            ScalarValue::Bool(v) => self.write_bool(v),
            ScalarValue::Char(c) => {
                let mut buf = [0u8; 4];
                self.write_str(c.encode_utf8(&mut buf));
            }
            ScalarValue::U64(n) => self.write_u64(n),
            ScalarValue::I64(n) => self.write_i64(n),
            ScalarValue::U128(n) => {
                // ASN.1 supports arbitrary-precision integers
                // For simplicity, convert to bytes
                if n <= u64::MAX as u128 {
                    self.write_u64(n as u64);
                } else {
                    let bytes = n.to_be_bytes();
                    let mut start = 0;
                    while start < 15 && bytes[start] == 0 {
                        start += 1;
                    }
                    // Ensure positive by checking high bit
                    if bytes[start] & 0x80 != 0 {
                        self.out.push(TAG_INTEGER);
                        self.write_length(bytes.len() - start + 1);
                        self.out.push(0x00);
                        self.out.extend_from_slice(&bytes[start..]);
                    } else {
                        self.write_tlv(TAG_INTEGER, &bytes[start..]);
                    }
                }
            }
            ScalarValue::I128(n) => {
                if n >= i64::MIN as i128 && n <= i64::MAX as i128 {
                    self.write_i64(n as i64);
                } else {
                    let bytes = n.to_be_bytes();
                    let mut leading_redundant = 0;
                    for window in bytes.windows(2) {
                        let byte = window[0] as i8;
                        let bit = window[1] as i8 >> 7;
                        if byte ^ bit == 0 {
                            leading_redundant += 1;
                        } else {
                            break;
                        }
                    }
                    self.write_tlv(TAG_INTEGER, &bytes[leading_redundant..]);
                }
            }
            ScalarValue::F64(n) => self.write_f64(n),
            ScalarValue::Str(s) => self.write_str(&s),
            ScalarValue::Bytes(bytes) => self.write_bytes(&bytes),
        }
        Ok(())
    }

    fn typed_scalar(
        &mut self,
        scalar_type: facet_core::ScalarType,
        value: facet_reflect::Peek<'_, '_>,
    ) -> Result<(), Self::Error> {
        use facet_core::ScalarType;

        // Handle unit type as an empty SEQUENCE (not NULL)
        // This allows roundtrip since the deserializer expects tuples to be sequences
        if matches!(scalar_type, ScalarType::Unit) {
            self.write_tlv(TAG_SEQUENCE | CONSTRUCTED_BIT, &[]);
            return Ok(());
        }

        // For other types, use the default implementation which calls scalar()
        let scalar = match scalar_type {
            ScalarType::Unit => unreachable!(), // Handled above
            ScalarType::Bool => ScalarValue::Bool(*value.get::<bool>().unwrap()),
            ScalarType::Char => {
                let c = *value.get::<char>().unwrap();
                let mut buf = [0u8; 4];
                ScalarValue::Str(alloc::borrow::Cow::Owned(
                    c.encode_utf8(&mut buf).to_string(),
                ))
            }
            ScalarType::Str | ScalarType::String | ScalarType::CowStr => {
                ScalarValue::Str(alloc::borrow::Cow::Borrowed(value.as_str().unwrap()))
            }
            ScalarType::F32 => ScalarValue::F64(*value.get::<f32>().unwrap() as f64),
            ScalarType::F64 => ScalarValue::F64(*value.get::<f64>().unwrap()),
            ScalarType::U8 => ScalarValue::U64(*value.get::<u8>().unwrap() as u64),
            ScalarType::U16 => ScalarValue::U64(*value.get::<u16>().unwrap() as u64),
            ScalarType::U32 => ScalarValue::U64(*value.get::<u32>().unwrap() as u64),
            ScalarType::U64 => ScalarValue::U64(*value.get::<u64>().unwrap()),
            ScalarType::U128 => ScalarValue::U128(*value.get::<u128>().unwrap()),
            ScalarType::USize => ScalarValue::U64(*value.get::<usize>().unwrap() as u64),
            ScalarType::I8 => ScalarValue::I64(*value.get::<i8>().unwrap() as i64),
            ScalarType::I16 => ScalarValue::I64(*value.get::<i16>().unwrap() as i64),
            ScalarType::I32 => ScalarValue::I64(*value.get::<i32>().unwrap() as i64),
            ScalarType::I64 => ScalarValue::I64(*value.get::<i64>().unwrap()),
            ScalarType::I128 => ScalarValue::I128(*value.get::<i128>().unwrap()),
            ScalarType::ISize => ScalarValue::I64(*value.get::<isize>().unwrap() as i64),
            _ => {
                // For unknown scalar types, try to get a string representation
                if let Some(s) = value.as_str() {
                    ScalarValue::Str(alloc::borrow::Cow::Borrowed(s))
                } else {
                    ScalarValue::Null
                }
            }
        };
        self.scalar(scalar)
    }
}

/// Serialize a value to ASN.1 DER bytes.
pub fn to_vec<'facet, T>(value: &T) -> Result<Vec<u8>, SerializeError<Asn1SerializeError>>
where
    T: facet_core::Facet<'facet>,
{
    let mut ser = Asn1Serializer::new();
    facet_format::serialize_root(&mut ser, facet_reflect::Peek::new(value))?;
    Ok(ser.finish())
}
