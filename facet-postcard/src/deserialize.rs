use crate::error::DeserializeError;
use facet_core::{Def, Facet, StructKind, Type, UserType};
use facet_reflect::Partial;
use log::trace;

#[cfg(feature = "alloc")]
use alloc::{borrow::Cow, string::String};

/// Deserializes postcard-encoded data into a type that implements `Facet`.
///
/// # Example
/// ```
/// use facet::Facet;
/// use facet_postcard::{from_bytes, to_vec};
///
/// #[derive(Debug, Facet, PartialEq)]
/// struct Point {
///     x: i32,
///     y: i32,
/// }
///
/// let original = Point { x: 10, y: 20 };
/// let bytes = to_vec(&original).unwrap();
/// let decoded: Point = from_bytes(&bytes).unwrap();
/// assert_eq!(original, decoded);
/// ```
pub fn from_bytes<T: Facet<'static>>(data: &[u8]) -> Result<T, DeserializeError> {
    trace!("from_bytes: Starting deserialization for type {}", T::SHAPE);
    let partial = Partial::alloc::<T>()?;
    trace!(
        "from_bytes: Allocated Partial, inner shape: {}",
        partial.shape()
    );

    let mut decoder = Decoder::new(data);
    let partial = decoder.deserialize_value(partial)?;

    trace!("from_bytes: Deserialization complete, building value");
    let heap_value = partial.build()?;
    trace!("from_bytes: Value built successfully");
    let value = heap_value.materialize()?;
    Ok(value)
}

/// Deserializes postcard-encoded data into a Facet value, returning the remaining bytes.
///
/// This is useful when you have multiple values concatenated in a buffer.
pub fn take_from_bytes<T: Facet<'static>>(data: &[u8]) -> Result<(T, &[u8]), DeserializeError> {
    trace!(
        "take_from_bytes: Starting deserialization for type {}",
        T::SHAPE
    );
    let partial = Partial::alloc::<T>()?;

    let mut decoder = Decoder::new(data);
    let partial = decoder.deserialize_value(partial)?;
    let remaining = decoder.remaining();

    let heap_value = partial.build()?;
    let value = heap_value.materialize()?;
    Ok((value, remaining))
}

struct Decoder<'input> {
    input: &'input [u8],
    offset: usize,
}

impl<'input> Decoder<'input> {
    fn new(input: &'input [u8]) -> Self {
        Decoder { input, offset: 0 }
    }

    fn remaining(&self) -> &'input [u8] {
        &self.input[self.offset..]
    }

    fn read_byte(&mut self) -> Result<u8, DeserializeError> {
        if self.offset >= self.input.len() {
            return Err(DeserializeError::UnexpectedEnd);
        }
        let value = self.input[self.offset];
        self.offset += 1;
        Ok(value)
    }

    fn read_bytes(&mut self, len: usize) -> Result<&'input [u8], DeserializeError> {
        if self.offset + len > self.input.len() {
            return Err(DeserializeError::UnexpectedEnd);
        }
        let value = &self.input[self.offset..self.offset + len];
        self.offset += len;
        Ok(value)
    }

    fn read_varint(&mut self) -> Result<u64, DeserializeError> {
        let mut result: u64 = 0;
        let mut shift = 0;
        loop {
            let byte = self.read_byte()?;
            result |= ((byte & 0x7F) as u64) << shift;
            if byte & 0x80 == 0 {
                break;
            }
            shift += 7;
            if shift >= 64 {
                return Err(DeserializeError::IntegerOverflow);
            }
        }
        Ok(result)
    }

    fn read_varint_u128(&mut self) -> Result<u128, DeserializeError> {
        let mut result: u128 = 0;
        let mut shift = 0;
        loop {
            let byte = self.read_byte()?;
            result |= ((byte & 0x7F) as u128) << shift;
            if byte & 0x80 == 0 {
                break;
            }
            shift += 7;
            if shift >= 128 {
                return Err(DeserializeError::IntegerOverflow);
            }
        }
        Ok(result)
    }

    fn read_varint_signed(&mut self) -> Result<i64, DeserializeError> {
        let encoded = self.read_varint()?;
        // Zigzag decoding: (encoded >> 1) ^ -(encoded & 1)
        let decoded = ((encoded >> 1) as i64) ^ -((encoded & 1) as i64);
        Ok(decoded)
    }

    fn read_varint_signed_i128(&mut self) -> Result<i128, DeserializeError> {
        let encoded = self.read_varint_u128()?;
        // Zigzag decoding: (encoded >> 1) ^ -(encoded & 1)
        let decoded = ((encoded >> 1) as i128) ^ -((encoded & 1) as i128);
        Ok(decoded)
    }

    fn read_bool(&mut self) -> Result<bool, DeserializeError> {
        match self.read_byte()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(DeserializeError::InvalidBool),
        }
    }

    fn read_string(&mut self) -> Result<String, DeserializeError> {
        let len = self.read_varint()? as usize;
        let bytes = self.read_bytes(len)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| DeserializeError::InvalidUtf8)
    }

    fn read_f32(&mut self) -> Result<f32, DeserializeError> {
        let bytes = self.read_bytes(4)?;
        Ok(f32::from_le_bytes(bytes.try_into().unwrap()))
    }

    fn read_f64(&mut self) -> Result<f64, DeserializeError> {
        let bytes = self.read_bytes(8)?;
        Ok(f64::from_le_bytes(bytes.try_into().unwrap()))
    }

    fn deserialize_value<'facet>(
        &mut self,
        partial: Partial<'facet>,
    ) -> Result<Partial<'facet>, DeserializeError> {
        let mut partial = partial;
        let shape = partial.shape();
        trace!("Deserializing {shape:?}");

        // First check the type system (Type)
        match &shape.ty {
            Type::User(UserType::Struct(struct_type)) if struct_type.kind != StructKind::Tuple => {
                trace!("Deserializing struct");
                // Postcard deserializes structs in field order
                for idx in 0..struct_type.fields.len() {
                    let field = &struct_type.fields[idx];
                    let field_partial = partial.begin_nth_field(idx)?;

                    // Skip fields marked with #[facet(skip)] or #[facet(skip_deserializing)]
                    let field_partial = if field.should_skip_deserializing() {
                        field_partial.set_default()?
                    } else {
                        self.deserialize_value(field_partial)?
                    };
                    partial = field_partial.end()?;
                }
                return Ok(partial);
            }
            Type::User(UserType::Struct(struct_type)) if struct_type.kind == StructKind::Tuple => {
                trace!("Deserializing tuple");
                for idx in 0..struct_type.fields.len() {
                    let field_partial = partial.begin_nth_field(idx)?;
                    let field_partial = self.deserialize_value(field_partial)?;
                    partial = field_partial.end()?;
                }
                return Ok(partial);
            }
            Type::User(UserType::Enum(_)) if matches!(shape.def, Def::Option(_)) => {
                // Option types are enums but need special handling via Def::Option path
                // Fall through to the Def::Option check below
            }
            Type::User(UserType::Enum(enum_type)) => {
                trace!("Deserializing enum");
                let variant_idx = self.read_varint()? as usize;

                if variant_idx >= enum_type.variants.len() {
                    return Err(DeserializeError::InvalidVariant);
                }

                let variant = &enum_type.variants[variant_idx];
                partial = partial.select_nth_variant(variant_idx)?;

                if variant.data.fields.is_empty() {
                    // Unit variant - nothing more to read
                    return Ok(partial);
                }

                // Deserialize variant fields in order
                for field_idx in 0..variant.data.fields.len() {
                    let field_partial = partial.begin_nth_field(field_idx)?;
                    let field_partial = self.deserialize_value(field_partial)?;
                    partial = field_partial.end()?;
                }

                return Ok(partial);
            }
            _ => {}
        }

        // Then check the def system (Def)
        if let Def::Scalar = shape.def {
            trace!("Deserializing scalar");
            if shape.is_type::<String>() {
                let s = self.read_string()?;
                partial = partial.set(s)?;
            } else if shape.is_type::<u64>() {
                let n = self.read_varint()?;
                partial = partial.set(n)?;
            } else if shape.is_type::<u32>() {
                let n = self.read_varint()?;
                if n > u32::MAX as u64 {
                    return Err(DeserializeError::IntegerOverflow);
                }
                partial = partial.set(n as u32)?;
            } else if shape.is_type::<u16>() {
                let n = self.read_varint()?;
                if n > u16::MAX as u64 {
                    return Err(DeserializeError::IntegerOverflow);
                }
                partial = partial.set(n as u16)?;
            } else if shape.is_type::<u8>() {
                let n = self.read_byte()?;
                partial = partial.set(n)?;
            } else if shape.is_type::<i64>() {
                let n = self.read_varint_signed()?;
                partial = partial.set(n)?;
            } else if shape.is_type::<i32>() {
                let n = self.read_varint_signed()?;
                if n > i32::MAX as i64 || n < i32::MIN as i64 {
                    return Err(DeserializeError::IntegerOverflow);
                }
                partial = partial.set(n as i32)?;
            } else if shape.is_type::<i16>() {
                let n = self.read_varint_signed()?;
                if n > i16::MAX as i64 || n < i16::MIN as i64 {
                    return Err(DeserializeError::IntegerOverflow);
                }
                partial = partial.set(n as i16)?;
            } else if shape.is_type::<i8>() {
                let n = self.read_byte()? as i8;
                partial = partial.set(n)?;
            } else if shape.is_type::<u128>() {
                let n = self.read_varint_u128()?;
                partial = partial.set(n)?;
            } else if shape.is_type::<i128>() {
                let n = self.read_varint_signed_i128()?;
                partial = partial.set(n)?;
            } else if shape.is_type::<usize>() {
                let n = self.read_varint()?;
                partial = partial.set(n as usize)?;
            } else if shape.is_type::<isize>() {
                let n = self.read_varint_signed()?;
                partial = partial.set(n as isize)?;
            } else if shape.is_type::<f32>() {
                let n = self.read_f32()?;
                partial = partial.set(n)?;
            } else if shape.is_type::<f64>() {
                let n = self.read_f64()?;
                partial = partial.set(n)?;
            } else if shape.is_type::<bool>() {
                let b = self.read_bool()?;
                partial = partial.set(b)?;
            } else if shape.is_type::<char>() {
                let s = self.read_string()?;
                let c = s.chars().next().ok_or(DeserializeError::InvalidData)?;
                partial = partial.set(c)?;
            } else if shape.is_type::<()>() {
                // Unit type - nothing to read
            } else if shape.is_type::<Cow<'_, str>>() {
                let s = self.read_string()?;
                partial = partial.set(Cow::Owned(s))?;
            } else {
                return Err(DeserializeError::UnsupportedType("Unknown scalar type"));
            }
        } else if let Def::Map(_map_def) = shape.def {
            trace!("Deserializing map");
            let map_len = self.read_varint()? as usize;
            partial = partial.begin_map()?;

            for _ in 0..map_len {
                let key_partial = partial.begin_key()?;
                let key_partial = self.deserialize_value(key_partial)?;
                partial = key_partial.end()?;

                let value_partial = partial.begin_value()?;
                let value_partial = self.deserialize_value(value_partial)?;
                partial = value_partial.end()?;
            }
        } else if let Def::List(list_def) = shape.def {
            trace!("Deserializing list");
            // Special case for Vec<u8>
            if list_def.t().is_type::<u8>() {
                let len = self.read_varint()? as usize;
                let bytes = self.read_bytes(len)?;
                partial = partial.set(bytes.to_vec())?;
            } else {
                let array_len = self.read_varint()? as usize;
                partial = partial.begin_list()?;

                for _ in 0..array_len {
                    let item_partial = partial.begin_list_item()?;
                    let item_partial = self.deserialize_value(item_partial)?;
                    partial = item_partial.end()?;
                }
            }
        } else if let Def::Array(array_def) = shape.def {
            trace!("Deserializing array");
            let expected_len = array_def.n;

            if expected_len == 0 {
                // Empty arrays need to be marked as initialized
                partial = partial.set_default()?;
            } else if array_def.t().is_type::<u8>() {
                // Special case for [u8; N]
                let bytes = self.read_bytes(expected_len)?;
                // For fixed byte arrays, set each element by index
                for (idx, &byte) in bytes.iter().enumerate() {
                    let item_partial = partial.begin_nth_field(idx)?;
                    let item_partial = item_partial.set(byte)?;
                    partial = item_partial.end()?;
                }
            } else {
                // Fixed-size arrays don't have length prefix in postcard
                // Use begin_nth_field for arrays (not begin_list which is for Vec)
                for idx in 0..expected_len {
                    let item_partial = partial.begin_nth_field(idx)?;
                    let item_partial = self.deserialize_value(item_partial)?;
                    partial = item_partial.end()?;
                }
            }
        } else if let Def::Set(_set_def) = shape.def {
            trace!("Deserializing set");
            let set_len = self.read_varint()? as usize;
            partial = partial.begin_set()?;

            for _ in 0..set_len {
                let item_partial = partial.begin_set_item()?;
                let item_partial = self.deserialize_value(item_partial)?;
                partial = item_partial.end()?;
            }
        } else if let Def::Option(_option_def) = shape.def {
            trace!("Deserializing option");
            let is_some = self.read_bool()?;
            if is_some {
                let some_partial = partial.begin_some()?;
                let some_partial = self.deserialize_value(some_partial)?;
                partial = some_partial.end()?;
            } else {
                partial = partial.set_default()?;
            }
        } else if let Def::Pointer(_) = shape.def {
            // Handle smart pointers by deserializing the inner value
            let inner = partial.begin_inner()?;
            let inner = self.deserialize_value(inner)?;
            partial = inner.end()?;
        } else {
            return Err(DeserializeError::UnsupportedShape);
        }

        Ok(partial)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::to_vec;
    use facet::Facet;
    use postcard::from_bytes as postcard_from_bytes;
    use serde::{Deserialize, Serialize};

    #[derive(Facet, Serialize, Deserialize, PartialEq, Debug)]
    struct SimpleStruct {
        a: u32,
        b: String,
        c: bool,
    }

    #[test]
    fn test_roundtrip_simple_struct() {
        facet_testhelpers::setup();

        let original = SimpleStruct {
            a: 123,
            b: "hello".to_string(),
            c: true,
        };

        let bytes = to_vec(&original).unwrap();
        let decoded: SimpleStruct = from_bytes(&bytes).unwrap();

        assert_eq!(original, decoded);
    }

    #[test]
    fn test_compatibility_with_postcard() {
        facet_testhelpers::setup();

        let original = SimpleStruct {
            a: 456,
            b: "world".to_string(),
            c: false,
        };

        // Serialize with facet-postcard, deserialize with postcard
        let facet_bytes = to_vec(&original).unwrap();
        let decoded: SimpleStruct = postcard_from_bytes(&facet_bytes).unwrap();
        assert_eq!(original, decoded);

        // Serialize with postcard, deserialize with facet-postcard
        let postcard_bytes = postcard::to_allocvec(&original).unwrap();
        let decoded: SimpleStruct = from_bytes(&postcard_bytes).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_roundtrip_option() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, Deserialize, PartialEq, Debug)]
        struct OptionStruct {
            some_value: Option<u32>,
            none_value: Option<u32>,
        }

        let original = OptionStruct {
            some_value: Some(42),
            none_value: None,
        };

        let bytes = to_vec(&original).unwrap();
        let decoded: OptionStruct = from_bytes(&bytes).unwrap();

        assert_eq!(original, decoded);
    }

    #[test]
    fn test_roundtrip_vec() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, Deserialize, PartialEq, Debug)]
        struct VecStruct {
            values: Vec<u32>,
        }

        let original = VecStruct {
            values: vec![1, 2, 3, 4, 5],
        };

        let bytes = to_vec(&original).unwrap();
        let decoded: VecStruct = from_bytes(&bytes).unwrap();

        assert_eq!(original, decoded);
    }

    #[test]
    fn test_roundtrip_enum() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, Deserialize, PartialEq, Debug)]
        #[repr(u8)]
        #[allow(dead_code)]
        enum TestEnum {
            Unit,
            Tuple(u32, String),
            Struct { x: i32, y: i32 },
        }

        // Test unit variant
        let original = TestEnum::Unit;
        let bytes = to_vec(&original).unwrap();
        let decoded: TestEnum = from_bytes(&bytes).unwrap();
        assert_eq!(original, decoded);

        // Test tuple variant
        let original = TestEnum::Tuple(42, "hello".to_string());
        let bytes = to_vec(&original).unwrap();
        let decoded: TestEnum = from_bytes(&bytes).unwrap();
        assert_eq!(original, decoded);

        // Test struct variant
        let original = TestEnum::Struct { x: 10, y: -20 };
        let bytes = to_vec(&original).unwrap();
        let decoded: TestEnum = from_bytes(&bytes).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_roundtrip_nested() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, Deserialize, PartialEq, Debug)]
        struct Inner {
            value: i32,
        }

        #[derive(Facet, Serialize, Deserialize, PartialEq, Debug)]
        struct Outer {
            inner: Inner,
            name: String,
        }

        let original = Outer {
            inner: Inner { value: -42 },
            name: "nested".to_string(),
        };

        let bytes = to_vec(&original).unwrap();
        let decoded: Outer = from_bytes(&bytes).unwrap();

        assert_eq!(original, decoded);
    }

    #[test]
    fn test_roundtrip_floats() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, Deserialize, PartialEq, Debug)]
        struct FloatStruct {
            f32_val: f32,
            f64_val: f64,
        }

        let original = FloatStruct {
            f32_val: 1.5,
            f64_val: 9.87654321,
        };

        let bytes = to_vec(&original).unwrap();
        let decoded: FloatStruct = from_bytes(&bytes).unwrap();

        assert_eq!(original, decoded);
    }

    #[test]
    fn test_take_from_bytes() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, Deserialize, PartialEq, Debug)]
        struct Point {
            x: i32,
            y: i32,
        }

        let point1 = Point { x: 1, y: 2 };
        let point2 = Point { x: 3, y: 4 };

        let mut bytes = to_vec(&point1).unwrap();
        bytes.extend(to_vec(&point2).unwrap());

        let (decoded1, remaining): (Point, _) = take_from_bytes(&bytes).unwrap();
        let (decoded2, remaining): (Point, _) = take_from_bytes(remaining).unwrap();

        assert_eq!(point1, decoded1);
        assert_eq!(point2, decoded2);
        assert!(remaining.is_empty());
    }
}
