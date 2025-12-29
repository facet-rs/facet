use crate::constants::*;
use crate::errors::Error as DecodeError;

use facet_core::{Def, Facet, Type, UserType};
use facet_reflect::Partial;
use log::trace;

/// Deserializes MessagePack-encoded data into a type that implements `Facet`.
///
/// # Example
/// ```
/// use facet::Facet;
/// use facet_msgpack_legacy::from_slice;
///
/// #[derive(Debug, Facet, PartialEq)]
/// struct User {
///     id: u64,
///     username: String,
/// }
///
/// // MessagePack binary data (equivalent to {"id": 42, "username": "user123"})
/// let msgpack_data = [
///     0x82, 0xa2, 0x69, 0x64, 0x2a, 0xa8, 0x75, 0x73,
///     0x65, 0x72, 0x6e, 0x61, 0x6d, 0x65, 0xa7, 0x75,
///     0x73, 0x65, 0x72, 0x31, 0x32, 0x33
/// ];
///
/// let user: User = from_slice(&msgpack_data).unwrap();
/// assert_eq!(user, User { id: 42, username: "user123".to_string() });
/// ```
pub fn from_slice<T: Facet<'static>>(msgpack: &[u8]) -> Result<T, DecodeError> {
    // FIXME: why is the bound `'static` up there? msgpack can borrow from the input afaict?

    trace!("from_slice: Starting deserialization for type {}", T::SHAPE);
    let partial = Partial::alloc::<T>()?;
    trace!(
        "from_slice: Allocated Partial, inner shape: {}",
        partial.shape()
    );

    let partial = from_slice_value(msgpack, partial)?;

    trace!("from_slice: Deserialization complete, building value");
    let heap_value = partial.build()?;
    trace!("from_slice: Value built successfully");
    let value = heap_value.materialize()?;
    Ok(value)
}

/// Deserializes MessagePack-encoded data into a Facet value.
///
/// This function takes a MessagePack byte array and populates a Partial object
/// according to the shape description, returning an Opaque value.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_msgpack_legacy::from_slice;
///
/// #[derive(Debug, Facet, PartialEq)]
/// struct User {
///     id: u64,
///     username: String,
/// }
///
/// // MessagePack binary data (equivalent to {"id": 42, "username": "user123"})
/// let msgpack_data = [
///     0x82, 0xa2, 0x69, 0x64, 0x2a, 0xa8, 0x75, 0x73,
///     0x65, 0x72, 0x6e, 0x61, 0x6d, 0x65, 0xa7, 0x75,
///     0x73, 0x65, 0x72, 0x31, 0x32, 0x33
/// ];
///
/// let user: User = from_slice(&msgpack_data).unwrap();
/// assert_eq!(user, User { id: 42, username: "user123".to_string() });
/// ```
///
/// # Parameters
/// * `wip` - A Partial object that will be filled with deserialized data
/// * `msgpack` - A byte slice containing MessagePack-encoded data
///
/// # Returns
/// * `Ok(Opaque)` containing the deserialized data if successful
/// * `Err(DecodeError)` if an error occurred during deserialization
///
/// # MessagePack Format
/// This implementation follows the MessagePack specification:
/// <https://github.com/msgpack/msgpack/blob/master/spec.md>
pub fn from_slice_value<'facet>(
    msgpack: &[u8],
    wip: Partial<'facet>,
) -> Result<Partial<'facet>, DecodeError> {
    trace!("from_slice_value: Starting with shape {}", wip.shape());
    let mut decoder = Decoder::new(msgpack);
    let result = decoder.deserialize_value(wip);
    match &result {
        Ok(_) => trace!("from_slice_value: Deserialization successful"),
        Err(e) => trace!("from_slice_value: Deserialization failed: {e:?}"),
    }
    result
}

struct Decoder<'input> {
    input: &'input [u8],
    offset: usize,
}

impl<'input> Decoder<'input> {
    fn new(input: &'input [u8]) -> Self {
        Decoder { input, offset: 0 }
    }

    /// Decodes a single byte from the input.
    /// This is a low-level method used by other decoders.
    fn decode_u8(&mut self) -> Result<u8, DecodeError> {
        if self.offset >= self.input.len() {
            return Err(DecodeError::InsufficientData);
        }
        let value = self.input[self.offset];
        self.offset += 1;
        Ok(value)
    }

    /// Decodes a 16-bit unsigned integer in big-endian byte order.
    /// This is a low-level method used by other decoders.
    fn decode_u16(&mut self) -> Result<u16, DecodeError> {
        if self.offset + 2 > self.input.len() {
            return Err(DecodeError::InsufficientData);
        }
        let value =
            u16::from_be_bytes(self.input[self.offset..self.offset + 2].try_into().unwrap());
        self.offset += 2;
        Ok(value)
    }

    /// Decodes a 32-bit unsigned integer in big-endian byte order.
    /// This is a low-level method used by other decoders.
    fn decode_u32(&mut self) -> Result<u32, DecodeError> {
        if self.offset + 4 > self.input.len() {
            return Err(DecodeError::InsufficientData);
        }
        let value =
            u32::from_be_bytes(self.input[self.offset..self.offset + 4].try_into().unwrap());
        self.offset += 4;
        Ok(value)
    }

    /// Decodes a MessagePack-encoded unsigned 64-bit integer.
    /// Handles the following MessagePack types:
    /// - positive fixint (0x00 - 0x7f): single-byte positive integer
    /// - uint8 (0xcc): 8-bit unsigned integer
    /// - uint16 (0xcd): 16-bit unsigned integer (big-endian)
    /// - uint32 (0xce): 32-bit unsigned integer (big-endian)
    /// - uint64 (0xcf): 64-bit unsigned integer (big-endian)
    ///
    /// Ref: <https://github.com/msgpack/msgpack/blob/master/spec.md#int-format-family>
    fn decode_u64(&mut self) -> Result<u64, DecodeError> {
        match self.decode_u8()? {
            MSGPACK_UINT8 => Ok(self.decode_u8()? as u64),
            MSGPACK_UINT16 => Ok(self.decode_u16()? as u64),
            MSGPACK_UINT32 => Ok(self.decode_u32()? as u64),
            MSGPACK_UINT64 => {
                if self.offset + 8 > self.input.len() {
                    return Err(DecodeError::InsufficientData);
                }
                let value = u64::from_be_bytes(
                    self.input[self.offset..self.offset + 8].try_into().unwrap(),
                );
                self.offset += 8;
                Ok(value)
            }
            prefix @ MSGPACK_POSFIXINT_MIN..=MSGPACK_POSFIXINT_MAX => Ok(prefix as u64),
            _ => Err(DecodeError::UnexpectedType),
        }
    }

    /// Decodes a MessagePack-encoded IEEE 754 single precision floating point number
    /// This is a low-level method used by other decoders.
    fn decode_f32(&mut self) -> Result<f32, DecodeError> {
        if self.offset + 4 > self.input.len() {
            return Err(DecodeError::InsufficientData);
        }
        let value =
            f32::from_be_bytes(self.input[self.offset..self.offset + 4].try_into().unwrap());
        self.offset += 4;
        Ok(value)
    }

    /// Decodes a MessagePack-encoded IEEE 754 double precision floating point number
    /// This is a low-level method used by other decoders.
    fn decode_f64(&mut self) -> Result<f64, DecodeError> {
        if self.offset + 8 > self.input.len() {
            return Err(DecodeError::InsufficientData);
        }
        let value =
            f64::from_be_bytes(self.input[self.offset..self.offset + 8].try_into().unwrap());
        self.offset += 8;
        Ok(value)
    }

    /// Decodes a MessagePack-encoded float.
    /// Handles the following MessagePack types:
    /// - f32 (0xca): 32-bit IEEE 754 single precision floating point number
    /// - f64 (0xcb): 64-bit IEEE 754 double precision floating point number
    ///
    /// Ref: <https://github.com/msgpack/msgpack/blob/master/spec.md#float-format-family>
    fn decode_float(&mut self) -> Result<f64, DecodeError> {
        match self.decode_u8()? {
            MSGPACK_FLOAT32 => self.decode_f32().map(f64::from),
            MSGPACK_FLOAT64 => self.decode_f64(),
            _ => Err(DecodeError::UnexpectedType),
        }
    }

    /// Decodes a MessagePack-encoded string.
    /// Handles the following MessagePack types:
    /// - fixstr (0xa0 - 0xbf): string up to 31 bytes
    /// - str8 (0xd9): string up to 255 bytes
    /// - str16 (0xda): string up to 65535 bytes
    /// - str32 (0xdb): string up to 4294967295 bytes
    ///
    /// Ref: <https://github.com/msgpack/msgpack/blob/master/spec.md#formats-str>
    fn decode_string(&mut self) -> Result<String, DecodeError> {
        let prefix = self.decode_u8()?;

        let len = match prefix {
            prefix @ MSGPACK_FIXSTR_MIN..=MSGPACK_FIXSTR_MAX => (prefix & 0x1f) as usize,
            MSGPACK_STR8 => self.decode_u8()? as usize,
            MSGPACK_STR16 => self.decode_u16()? as usize,
            MSGPACK_STR32 => self.decode_u32()? as usize,
            _ => return Err(DecodeError::UnexpectedType),
        };

        if self.offset + len > self.input.len() {
            return Err(DecodeError::InsufficientData);
        }

        let value = String::from_utf8(self.input[self.offset..self.offset + len].to_vec())
            .map_err(|_| DecodeError::InvalidData)?;
        self.offset += len;
        Ok(value)
    }

    /// Decodes a MessagePack-encoded map length.
    /// Handles the following MessagePack types:
    /// - fixmap (0x80 - 0x8f): map with up to 15 elements
    /// - map16 (0xde): map with up to 65535 elements
    /// - map32 (0xdf): map with up to 4294967295 elements
    ///
    /// Ref: <https://github.com/msgpack/msgpack/blob/master/spec.md#formats-map>
    fn decode_map_len(&mut self) -> Result<usize, DecodeError> {
        let prefix = self.decode_u8()?;

        match prefix {
            prefix @ MSGPACK_FIXMAP_MIN..=MSGPACK_FIXMAP_MAX => Ok((prefix & 0x0f) as usize),
            MSGPACK_MAP16 => Ok(self.decode_u16()? as usize),
            MSGPACK_MAP32 => Ok(self.decode_u32()? as usize),
            _ => Err(DecodeError::UnexpectedType),
        }
    }

    /// Decodes a MessagePack-encoded array length.
    /// Handles the following MessagePack types:
    /// - fixarray (0x90 - 0x9f): array with up to 15 elements
    /// - array16 (0xdc): array with up to 65535 elements
    /// - array32 (0xdd): array with up to 4294967295 elements
    ///
    /// Ref: <https://github.com/msgpack/msgpack/blob/master/spec.md#formats-array>
    #[allow(dead_code)]
    fn decode_array_len(&mut self) -> Result<usize, DecodeError> {
        let prefix = self.decode_u8()?;

        match prefix {
            prefix @ MSGPACK_FIXARRAY_MIN..=MSGPACK_FIXARRAY_MAX => Ok((prefix & 0x0f) as usize),
            MSGPACK_ARRAY16 => Ok(self.decode_u16()? as usize),
            MSGPACK_ARRAY32 => Ok(self.decode_u32()? as usize),
            _ => Err(DecodeError::UnexpectedType),
        }
    }

    /// Decodes a MessagePack-encoded boolean value.
    /// Handles the following MessagePack types:
    /// - true (0xc3): boolean true
    /// - false (0xc2): boolean false
    ///
    /// Ref: <https://github.com/msgpack/msgpack/blob/master/spec.md#formats-bool>
    fn decode_bool(&mut self) -> Result<bool, DecodeError> {
        match self.decode_u8()? {
            MSGPACK_TRUE => Ok(true),
            MSGPACK_FALSE => Ok(false),
            _ => Err(DecodeError::UnexpectedType),
        }
    }

    /// Decodes a MessagePack-encoded nil value.
    /// Handles the following MessagePack types:
    /// - nil (0xc0): nil/null value
    ///
    /// Ref: <https://github.com/msgpack/msgpack/blob/master/spec.md#formats-nil>
    #[allow(dead_code)]
    fn decode_nil(&mut self) -> Result<(), DecodeError> {
        match self.decode_u8()? {
            MSGPACK_NIL => Ok(()),
            _ => Err(DecodeError::UnexpectedType),
        }
    }

    /// Peeks at the next byte to check if it's a nil value without advancing the offset.
    /// Returns true if the next value is nil, false otherwise.
    #[allow(dead_code)]
    fn peek_nil(&mut self) -> Result<bool, DecodeError> {
        if self.offset >= self.input.len() {
            return Err(DecodeError::InsufficientData);
        }
        Ok(self.input[self.offset] == MSGPACK_NIL)
    }

    /// Peeks at the next byte to check if it's a string value without advancing the offset.
    /// Returns true if the next value is a string, false otherwise.
    fn peek_string(&mut self) -> Result<bool, DecodeError> {
        if self.offset >= self.input.len() {
            return Err(DecodeError::InsufficientData);
        }
        let prefix = self.input[self.offset];
        Ok((MSGPACK_FIXSTR_MIN..=MSGPACK_FIXSTR_MAX).contains(&prefix)
            || prefix == MSGPACK_STR8
            || prefix == MSGPACK_STR16
            || prefix == MSGPACK_STR32)
    }

    /// Skips a MessagePack value of any type.
    /// This is used when encountering unknown field names in a struct.
    fn skip_value(&mut self) -> Result<(), DecodeError> {
        let prefix = self.decode_u8()?;

        match prefix {
            // String formats
            prefix @ MSGPACK_FIXSTR_MIN..=MSGPACK_FIXSTR_MAX => {
                let len = (prefix & 0x1f) as usize;
                if self.offset + len > self.input.len() {
                    return Err(DecodeError::InsufficientData);
                }
                self.offset += len;
                Ok(())
            }
            MSGPACK_STR8 => {
                let len = self.decode_u8()? as usize;
                if self.offset + len > self.input.len() {
                    return Err(DecodeError::InsufficientData);
                }
                self.offset += len;
                Ok(())
            }
            MSGPACK_STR16 => {
                let len = self.decode_u16()? as usize;
                if self.offset + len > self.input.len() {
                    return Err(DecodeError::InsufficientData);
                }
                self.offset += len;
                Ok(())
            }
            MSGPACK_STR32 => {
                let len = self.decode_u32()? as usize;
                if self.offset + len > self.input.len() {
                    return Err(DecodeError::InsufficientData);
                }
                self.offset += len;
                Ok(())
            }

            // Integer formats
            MSGPACK_UINT8 => {
                self.offset += 1;
                Ok(())
            }
            MSGPACK_UINT16 => {
                self.offset += 2;
                Ok(())
            }
            MSGPACK_UINT32 => {
                self.offset += 4;
                Ok(())
            }
            MSGPACK_UINT64 => {
                self.offset += 8;
                Ok(())
            }
            MSGPACK_INT8 => {
                self.offset += 1;
                Ok(())
            }
            MSGPACK_INT16 => {
                self.offset += 2;
                Ok(())
            }
            MSGPACK_INT32 => {
                self.offset += 4;
                Ok(())
            }
            MSGPACK_INT64 => {
                self.offset += 8;
                Ok(())
            }
            // Fixed integers are already handled by decode_u8

            // Float formats
            MSGPACK_FLOAT32 => {
                self.offset += 4;
                Ok(())
            }
            MSGPACK_FLOAT64 => {
                self.offset += 8;
                Ok(())
            }

            // Boolean and nil
            MSGPACK_NIL | MSGPACK_TRUE | MSGPACK_FALSE => Ok(()),

            // Map format
            prefix @ MSGPACK_FIXMAP_MIN..=MSGPACK_FIXMAP_MAX => {
                let len = (prefix & 0x0f) as usize;
                for _ in 0..len {
                    self.skip_value()?; // Skip key
                    self.skip_value()?; // Skip value
                }
                Ok(())
            }
            MSGPACK_MAP16 => {
                let len = self.decode_u16()? as usize;
                for _ in 0..len {
                    self.skip_value()?; // Skip key
                    self.skip_value()?; // Skip value
                }
                Ok(())
            }
            MSGPACK_MAP32 => {
                let len = self.decode_u32()? as usize;
                for _ in 0..len {
                    self.skip_value()?; // Skip key
                    self.skip_value()?; // Skip value
                }
                Ok(())
            }

            // Array format
            prefix @ MSGPACK_FIXARRAY_MIN..=MSGPACK_FIXARRAY_MAX => {
                let len = (prefix & 0x0f) as usize;
                for _ in 0..len {
                    self.skip_value()?;
                }
                Ok(())
            }
            MSGPACK_ARRAY16 => {
                let len = self.decode_u16()? as usize;
                for _ in 0..len {
                    self.skip_value()?;
                }
                Ok(())
            }
            MSGPACK_ARRAY32 => {
                let len = self.decode_u32()? as usize;
                for _ in 0..len {
                    self.skip_value()?;
                }
                Ok(())
            }

            _ => Err(DecodeError::UnexpectedType),
        }
    }

    fn deserialize_value<'facet>(
        &mut self,
        partial: Partial<'facet>,
    ) -> Result<Partial<'facet>, DecodeError> {
        let mut partial = partial;
        let shape = partial.shape();
        trace!("Deserializing {shape:?}");

        // First check the type system (Type)
        match &shape.ty {
            Type::User(UserType::Struct(struct_type))
                if struct_type.kind != facet_core::StructKind::Tuple =>
            {
                trace!("Deserializing struct");
                let map_len = self.decode_map_len()?;

                // Track which fields we've seen so we can handle defaults for the rest
                let mut seen_fields = vec![false; struct_type.fields.len()];

                for _ in 0..map_len {
                    let key = self.decode_string()?;
                    match partial.field_index(&key) {
                        Some(index) => {
                            seen_fields[index] = true;
                            let field_partial = partial.begin_nth_field(index).unwrap();
                            let field_partial = self.deserialize_value(field_partial)?;
                            partial = field_partial.end().unwrap();
                        }
                        None => {
                            // Skip unknown field value
                            self.skip_value()?;
                            trace!("Skipping unknown field: {key}");
                        }
                    }
                }

                // Handle defaults for fields that weren't seen in the input
                for (i, &seen) in seen_fields.iter().enumerate() {
                    if !seen {
                        let field = &struct_type.fields[i];
                        if field.has_default() {
                            partial = partial.set_nth_field_to_default(i)?;
                        } else {
                            // Non-default field was missing
                            return Err(DecodeError::MissingField(field.name.to_string()));
                        }
                    }
                }

                return Ok(partial);
            }
            Type::User(facet_core::UserType::Struct(struct_type))
                if struct_type.kind == facet_core::StructKind::Tuple =>
            {
                trace!("Deserializing tuple");
                let array_len = self.decode_array_len()?;
                let field_count = struct_type.fields.len();

                if array_len != field_count {
                    return Err(DecodeError::InvalidData);
                }

                // For tuples, deserialize fields in order
                for idx in 0..field_count {
                    trace!("Deserializing tuple field {idx}");
                    let field_partial = partial.begin_nth_field(idx)?;
                    let field_partial = self.deserialize_value(field_partial)?;
                    partial = field_partial.end().map_err(DecodeError::ReflectError)?;
                }

                return Ok(partial);
            }
            Type::User(UserType::Enum(enum_type)) => {
                trace!("Deserializing enum");

                // Check if it's a unit variant which is represented as a string
                if self.peek_string()? {
                    let variant_name = self.decode_string()?;
                    for (idx, variant) in enum_type.variants.iter().enumerate() {
                        if variant.name == variant_name {
                            partial = partial.select_nth_variant(idx)?;
                            return Ok(partial);
                        }
                    }
                    return Err(DecodeError::InvalidEnum(format!(
                        "Unknown variant: {variant_name}"
                    )));
                }

                // Otherwise it's represented as a map with single entry where key is the variant name
                let map_len = self.decode_map_len()?;
                if map_len != 1 {
                    return Err(DecodeError::InvalidData);
                }

                let variant_name = self.decode_string()?;

                for (idx, variant) in enum_type.variants.iter().enumerate() {
                    if variant.name == variant_name {
                        match &variant.data.kind {
                            // Handle unit variant
                            facet_core::StructKind::Unit => {
                                // Need to skip any value that might be present
                                self.skip_value()?;
                                partial = partial.select_nth_variant(idx)?;
                                return Ok(partial);
                            }

                            // Handle tuple variant
                            facet_core::StructKind::Tuple => {
                                let array_len = self.decode_array_len()?;
                                let field_count = variant.data.fields.len();

                                if array_len != field_count {
                                    return Err(DecodeError::InvalidData);
                                }

                                partial = partial.select_nth_variant(idx)?;
                                for field_idx in 0..field_count {
                                    let field_partial = partial.begin_nth_field(field_idx)?;
                                    let field_partial = self.deserialize_value(field_partial)?;
                                    partial = field_partial.end()?;
                                }
                                return Ok(partial);
                            }

                            // Handle struct variant
                            facet_core::StructKind::Struct => {
                                let map_len = self.decode_map_len()?;
                                partial = partial.select_nth_variant(idx)?;

                                // Handle fields as a normal struct
                                for _ in 0..map_len {
                                    let field_name = self.decode_string()?;
                                    match partial.field_index(&field_name) {
                                        Some(field_idx) => {
                                            let field_partial =
                                                partial.begin_nth_field(field_idx)?;
                                            let field_partial =
                                                self.deserialize_value(field_partial)?;
                                            partial = field_partial.end()?;
                                        }
                                        None => {
                                            // Skip unknown field
                                            self.skip_value()?;
                                            trace!("Skipping unknown field in enum: {field_name}");
                                        }
                                    }
                                }

                                return Ok(partial);
                            }

                            // Handle other kinds that might be added in the future
                            _ => {
                                return Err(DecodeError::UnsupportedType(format!(
                                    "Unsupported enum variant kind: {:?}",
                                    variant.data.kind
                                )));
                            }
                        }
                    }
                }

                return Err(DecodeError::InvalidEnum(format!(
                    "Unknown variant: {variant_name}"
                )));
            }
            _ => {}
        }

        // Then check the def system (Def)
        if let Def::Scalar = shape.def {
            trace!("Deserializing scalar");
            if shape.is_type::<String>() {
                let s = self.decode_string()?;
                partial = partial.set(s)?;
            } else if shape.is_type::<u64>() {
                let n = self.decode_u64()?;
                partial = partial.set(n)?;
            } else if shape.is_type::<u32>() {
                let n = self.decode_u64()?;
                if n > u32::MAX as u64 {
                    return Err(DecodeError::IntegerOverflow);
                }
                partial = partial.set(n as u32)?;
            } else if shape.is_type::<u16>() {
                let n = self.decode_u64()?;
                if n > u16::MAX as u64 {
                    return Err(DecodeError::IntegerOverflow);
                }
                partial = partial.set(n as u16)?;
            } else if shape.is_type::<u8>() {
                let n = self.decode_u64()?;
                if n > u8::MAX as u64 {
                    return Err(DecodeError::IntegerOverflow);
                }
                partial = partial.set(n as u8)?;
            } else if shape.is_type::<i64>() {
                // TODO: implement proper signed int decoding including negative values
                let n = self.decode_u64()?;
                if n > i64::MAX as u64 {
                    return Err(DecodeError::IntegerOverflow);
                }
                partial = partial.set(n as i64)?;
            } else if shape.is_type::<i32>() {
                let n = self.decode_u64()?;
                if n > i32::MAX as u64 {
                    return Err(DecodeError::IntegerOverflow);
                }
                partial = partial.set(n as i32)?;
            } else if shape.is_type::<i16>() {
                let n = self.decode_u64()?;
                if n > i16::MAX as u64 {
                    return Err(DecodeError::IntegerOverflow);
                }
                partial = partial.set(n as i16)?;
            } else if shape.is_type::<i8>() {
                let n = self.decode_u64()?;
                if n > i8::MAX as u64 {
                    return Err(DecodeError::IntegerOverflow);
                }
                partial = partial.set(n as i8)?;
            } else if shape.is_type::<f64>() {
                let n = self.decode_float()?;
                partial = partial.set(n)?;
            } else if shape.is_type::<f32>() {
                let n = self.decode_float()?;
                let n_as_f32 = std::hint::black_box(n as f32);
                if n_as_f32 as f64 != n {
                    return Err(DecodeError::FloatOverflow);
                }
                partial = partial.set(n_as_f32)?;
            } else if shape.is_type::<bool>() {
                let b = self.decode_bool()?;
                partial = partial.set(b)?;
            } else {
                return Err(DecodeError::UnsupportedType(format!("{shape}")));
            }
        } else if let Def::Map(_map_def) = shape.def {
            trace!("Deserializing map");
            let map_len = self.decode_map_len()?;
            partial = partial.begin_map()?;

            for _ in 0..map_len {
                // Each map entry has a key and value
                let key_partial = partial.begin_key()?;
                let key_partial = self.deserialize_value(key_partial)?;
                partial = key_partial.end()?;

                let value_partial = partial.begin_value()?;
                let value_partial = self.deserialize_value(value_partial)?;
                partial = value_partial.end()?;
            }
        } else if let Def::List(_list_def) = shape.def {
            trace!("Deserializing list");
            let array_len = self.decode_array_len()?;
            partial = partial.begin_list()?;

            for _ in 0..array_len {
                let item_partial = partial.begin_list_item()?;
                let item_partial = self.deserialize_value(item_partial)?;
                partial = item_partial.end()?;
            }
        } else if let Def::Option(_option_def) = shape.def {
            trace!("Deserializing option with shape: {shape}");
            if self.peek_nil()? {
                trace!("Option value is nil, setting to None");
                // Consume the nil value
                self.decode_nil()?;
                // Initialize None option
                partial = partial.set_default()?;
            } else {
                trace!("Option value is present, setting to Some");
                // Value is present - initialize a Some option
                let some_partial = partial.begin_some()?;
                trace!("After begin_some, partial shape: {}", some_partial.shape());
                let some_partial = self.deserialize_value(some_partial)?;
                trace!("After deserialize_value, calling end");
                partial = some_partial.end()?;
                trace!("After end, partial shape: {}", partial.shape());
            }
        } else {
            return Err(DecodeError::UnsupportedShape(format!("{shape:?}")));
        }

        Ok(partial)
    }
}
