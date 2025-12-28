//! CSV serialization implementation using FormatSerializer trait.

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

use facet_core::Facet;
use facet_format::{FormatSerializer, ScalarValue, SerializeError, serialize_root};
use facet_reflect::Peek;

/// Error type for CSV serialization.
#[derive(Debug)]
pub struct CsvSerializeError {
    msg: &'static str,
}

impl core::fmt::Display for CsvSerializeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.msg)
    }
}

impl std::error::Error for CsvSerializeError {}

/// CSV serializer implementing FormatSerializer.
pub struct CsvSerializer {
    out: Vec<u8>,
    in_struct: bool,
    first_field: bool,
}

impl CsvSerializer {
    /// Create a new CSV serializer.
    pub fn new() -> Self {
        Self {
            out: Vec::new(),
            in_struct: false,
            first_field: true,
        }
    }

    /// Consume the serializer and return the output bytes.
    pub fn finish(self) -> Vec<u8> {
        self.out
    }

    fn write_csv_escaped(&mut self, s: &str) {
        // Check if we need to quote the field
        let needs_quoting =
            s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r');

        if needs_quoting {
            self.out.push(b'"');
            for c in s.chars() {
                if c == '"' {
                    self.out.extend_from_slice(b"\"\"");
                } else {
                    let mut buf = [0u8; 4];
                    let len = c.encode_utf8(&mut buf).len();
                    self.out.extend_from_slice(&buf[..len]);
                }
            }
            self.out.push(b'"');
        } else {
            self.out.extend_from_slice(s.as_bytes());
        }
    }
}

impl Default for CsvSerializer {
    fn default() -> Self {
        Self::new()
    }
}

impl FormatSerializer for CsvSerializer {
    type Error = CsvSerializeError;

    fn begin_struct(&mut self) -> Result<(), Self::Error> {
        if self.in_struct {
            return Err(CsvSerializeError {
                msg: "CSV does not support nested structures",
            });
        }
        self.in_struct = true;
        self.first_field = true;
        Ok(())
    }

    fn field_key(&mut self, _key: &str) -> Result<(), Self::Error> {
        // CSV doesn't output field names, just values
        // But we need to add comma separators between fields
        if !self.first_field {
            self.out.push(b',');
        }
        self.first_field = false;
        Ok(())
    }

    fn end_struct(&mut self) -> Result<(), Self::Error> {
        self.in_struct = false;
        // Add newline at end of row
        self.out.push(b'\n');
        Ok(())
    }

    fn begin_seq(&mut self) -> Result<(), Self::Error> {
        Err(CsvSerializeError {
            msg: "CSV does not support sequences",
        })
    }

    fn end_seq(&mut self) -> Result<(), Self::Error> {
        Err(CsvSerializeError {
            msg: "CSV does not support sequences",
        })
    }

    fn scalar(&mut self, scalar: ScalarValue<'_>) -> Result<(), Self::Error> {
        match scalar {
            ScalarValue::Null => {
                // Empty field for null
            }
            ScalarValue::Bool(v) => {
                if v {
                    self.out.extend_from_slice(b"true");
                } else {
                    self.out.extend_from_slice(b"false");
                }
            }
            ScalarValue::I64(v) => {
                self.out.extend_from_slice(v.to_string().as_bytes());
            }
            ScalarValue::U64(v) => {
                self.out.extend_from_slice(v.to_string().as_bytes());
            }
            ScalarValue::I128(v) => {
                self.out.extend_from_slice(v.to_string().as_bytes());
            }
            ScalarValue::U128(v) => {
                self.out.extend_from_slice(v.to_string().as_bytes());
            }
            ScalarValue::F64(v) => {
                self.out.extend_from_slice(v.to_string().as_bytes());
            }
            ScalarValue::Str(s) => {
                self.write_csv_escaped(&s);
            }
            ScalarValue::Bytes(_) => {
                return Err(CsvSerializeError {
                    msg: "CSV does not support binary data",
                });
            }
        }
        Ok(())
    }
}

/// Serialize a value to CSV bytes.
pub fn to_vec<'facet, T>(value: &T) -> Result<Vec<u8>, SerializeError<CsvSerializeError>>
where
    T: Facet<'facet> + ?Sized,
{
    let mut serializer = CsvSerializer::new();
    serialize_root(&mut serializer, Peek::new(value))?;
    Ok(serializer.finish())
}

/// Serialize a value to a CSV string.
pub fn to_string<'facet, T>(value: &T) -> Result<String, SerializeError<CsvSerializeError>>
where
    T: Facet<'facet> + ?Sized,
{
    let bytes = to_vec(value)?;
    Ok(String::from_utf8(bytes).expect("CSV output should always be valid UTF-8"))
}

/// Serialize a value to a writer in CSV format.
pub fn to_writer<'facet, W, T>(writer: &mut W, value: &T) -> std::io::Result<()>
where
    W: std::io::Write,
    T: Facet<'facet> + ?Sized,
{
    let bytes = to_vec(value).map_err(|e| std::io::Error::other(alloc::format!("{:?}", e)))?;
    writer.write_all(&bytes)
}
