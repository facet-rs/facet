use crate::constants::*;
use crate::errors::Error as DecodeError;

use facet_core::{Facet, Opaque};
use facet_reflect::PokeUninit;
use log::trace;

/// Deserializes MessagePack-encoded data into a type that implements `Facet`.
///
/// # Example
/// ```
/// use facet::Facet;
/// use facet_msgpack::from_str;
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
/// let user: User = from_str(&msgpack_data).unwrap();
/// assert_eq!(user, User { id: 42, username: "user123".to_string() });
/// ```
pub fn from_str<T: Facet>(msgpack: &[u8]) -> Result<T, DecodeError> {
    // Allocate a Poke for type T
    let (poke, _guard) = PokeUninit::alloc::<T>();

    // Deserialize the MessagePack into the Poke
    let opaque = from_slice_opaque(poke, msgpack)?;

    // Convert the Opaque to the concrete type T
    let result = unsafe { opaque.read::<T>() };

    Ok(result)
}

/// Deserializes MessagePack-encoded data into a Facet Partial.
///
/// This function takes a MessagePack byte array and populates a Partial object
/// according to the shape description provided by the Partial.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_msgpack::from_str;
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
/// let user: User = from_str(&msgpack_data).unwrap();
/// assert_eq!(user, User { id: 42, username: "user123".to_string() });
/// ```
///
/// # Parameters
/// * `partial` - A mutable reference to a Partial object that will be filled with deserialized data
/// * `msgpack` - A byte slice containing MessagePack-encoded data
///
/// # Returns
/// * `Ok(())` if deserialization was successful
/// * `Err(DecodeError)` if an error occurred during deserialization
///
/// # MessagePack Format
/// This implementation follows the MessagePack specification:
/// <https://github.com/msgpack/msgpack/blob/master/spec.md>
#[allow(clippy::needless_lifetimes)]
pub fn from_slice_opaque<'mem>(
    poke: PokeUninit<'mem>,
    msgpack: &[u8],
) -> Result<Opaque<'mem>, DecodeError> {
    let mut decoder = Decoder::new(msgpack);

    fn deserialize_value<'mem>(
        decoder: &mut Decoder,
        poke: PokeUninit<'mem>,
    ) -> Result<Opaque<'mem>, DecodeError> {
        let shape = poke.shape();
        trace!("Deserializing {:?}", shape);

        let opaque = match poke {
            PokeUninit::Scalar(pv) => {
                trace!("Deserializing scalar");
                if pv.shape().is_type::<String>() {
                    let s = decoder.decode_string()?;
                    let data = pv.put(s);
                    data
                } else if pv.shape().is_type::<u64>() {
                    let n = decoder.decode_u64()?;
                    pv.put(n)
                } else {
                    todo!("Unsupported scalar type: {}", pv.shape())
                }
                .data()
            }
            PokeUninit::Struct(mut ps) => {
                trace!("Deserializing struct");
                let map_len = decoder.decode_map_len()?;

                for _ in 0..map_len {
                    let key = decoder.decode_string()?;
                    let (index, field_poke) = ps
                        .field_by_name(&key)
                        .map_err(|_| DecodeError::UnknownField(key))?;

                    deserialize_value(decoder, field_poke)?;
                    unsafe { ps.mark_initialized(index) };
                }
                ps.build_in_place()
            }
            _ => {
                todo!("Unsupported shape: {:?}", shape)
            }
        };

        Ok(opaque)
    }

    deserialize_value(&mut decoder, poke)
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
}
