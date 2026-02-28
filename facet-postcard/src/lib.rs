//! Postcard binary format for facet.
//!
//! This crate provides serialization and deserialization for the postcard binary format.
//!
//! # Serialization
//!
//! Serialization supports all types that implement [`facet_core::Facet`]:
//!
//! ```
//! use facet::Facet;
//! use facet_postcard::to_vec;
//!
//! #[derive(Facet)]
//! struct Point { x: i32, y: i32 }
//!
//! let point = Point { x: 10, y: 20 };
//! let bytes = to_vec(&point).unwrap();
//! ```
//!
//! # Deserialization
//!
//! There is a configurable [`Deserializer`] API plus convenience functions:
//!
//! - [`from_slice`]: Deserializes into owned types (`T: Facet<'static>`)
//! - [`from_slice_borrowed`]: Deserializes with zero-copy borrowing from the input buffer
//! - [`from_slice_with_shape`]: Deserializes into `Value` using runtime shape information
//! - [`from_slice_into`]: Deserializes into an existing `Partial` (type-erased, owned)
//! - [`from_slice_into_borrowed`]: Deserializes into an existing `Partial` (type-erased, zero-copy)
//!
//! ```
//! use facet_postcard::from_slice;
//!
//! // Postcard encoding: [length=3, true, false, true]
//! let bytes = &[0x03, 0x01, 0x00, 0x01];
//! let result: Vec<bool> = from_slice(bytes).unwrap();
//! assert_eq!(result, vec![true, false, true]);
//! ```
//!
//! Both functions automatically select the best deserialization tier:
//! - **Tier-2 (Format JIT)**: Fastest path for compatible types (primitives, structs, vecs, simple enums)
//! - **Tier-0 (Reflection)**: Fallback for all other types (nested enums, complex types)
//!
//! This ensures all `Facet` types can be deserialized.

// Note: unsafe code is used for lifetime transmutes in from_slice_into
// when BORROW=false, mirroring the approach used in facet-json.

extern crate alloc;

mod error;
mod parser;
mod serialize;
mod shape_deser;

#[cfg(feature = "jit")]
pub mod jit;

#[cfg(feature = "axum")]
mod axum;

#[cfg(feature = "axum")]
pub use axum::{Postcard, PostcardRejection, PostcardSerializeRejection};
pub use error::{PostcardError, SerializeError};
#[cfg(feature = "jit")]
pub use jit::PostcardJitFormat;
pub use parser::PostcardParser;
pub use serialize::{
    ScatterPlan, Segment, Writer, peek_to_scatter_plan, peek_to_vec, to_scatter_plan, to_vec,
    to_vec_with_shape, to_writer_fallible,
};
pub use shape_deser::from_slice_with_shape;

// Re-export DeserializeError for convenience
pub use facet_format::DeserializeError;

// Postcard-specific facet attributes.
facet::define_attr_grammar! {
    ns "postcard";
    crate_path ::facet_postcard;

    pub enum Attr {
        /// Marks an opaque field as trailing in postcard, so its mapped payload
        /// is serialized without an outer byte-sequence length wrapper.
        Trailing,
    }
}

/// Default maximum number of elements allowed in a decoded collection.
///
/// This limit applies to postcard length-prefixed collections (lists, maps,
/// dynamic arrays/objects) and is enforced in both Tier-0 and Tier-2 JIT paths.
pub const DEFAULT_MAX_COLLECTION_ELEMENTS: u64 = 1 << 24; // 16,777,216

/// Deserialization safety/configuration options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeserializeConfig {
    max_collection_elements: u64,
}

impl Default for DeserializeConfig {
    fn default() -> Self {
        Self {
            max_collection_elements: DEFAULT_MAX_COLLECTION_ELEMENTS,
        }
    }
}

impl DeserializeConfig {
    /// Create default deserialization settings.
    pub const fn new() -> Self {
        Self {
            max_collection_elements: DEFAULT_MAX_COLLECTION_ELEMENTS,
        }
    }

    /// Set the maximum number of elements permitted in any decoded collection.
    pub const fn max_collection_elements(mut self, max_collection_elements: u64) -> Self {
        self.max_collection_elements = max_collection_elements;
        self
    }

    /// Get the configured maximum number of collection elements.
    pub const fn get_max_collection_elements(self) -> u64 {
        self.max_collection_elements
    }
}

/// Builder-style postcard deserializer.
///
/// This single API supports all current entry points:
/// typed owned/borrowed deserialization, shape-based value deserialization,
/// and deserialization into existing `Partial` values.
#[derive(Debug, Clone, Copy)]
pub struct Deserializer<'input> {
    input: &'input [u8],
    config: DeserializeConfig,
}

impl<'input> Deserializer<'input> {
    /// Create a deserializer for a postcard byte slice with default settings.
    pub const fn new(input: &'input [u8]) -> Self {
        Self {
            input,
            config: DeserializeConfig::new(),
        }
    }

    /// Create a deserializer with explicit settings.
    pub const fn with_config(input: &'input [u8], config: DeserializeConfig) -> Self {
        Self { input, config }
    }

    /// Replace all deserialization settings.
    pub const fn config(mut self, config: DeserializeConfig) -> Self {
        self.config = config;
        self
    }

    /// Configure the maximum collection element count.
    pub const fn max_collection_elements(mut self, max_collection_elements: u64) -> Self {
        self.config = self.config.max_collection_elements(max_collection_elements);
        self
    }

    fn parser(self) -> PostcardParser<'input> {
        PostcardParser::with_limits(self.input, self.config.get_max_collection_elements())
    }

    /// Deserialize into an owned typed value.
    pub fn deserialize<T>(self) -> Result<T, DeserializeError>
    where
        T: facet_core::Facet<'static>,
    {
        use facet_format::FormatDeserializer;
        let mut parser = self.parser();
        let mut de = FormatDeserializer::new_owned(&mut parser);
        de.deserialize()
    }

    /// Deserialize into a borrowed typed value.
    pub fn deserialize_borrowed<'facet, T>(self) -> Result<T, DeserializeError>
    where
        T: facet_core::Facet<'facet>,
        'input: 'facet,
    {
        use facet_format::FormatDeserializer;
        let mut parser = self.parser();
        let mut de = FormatDeserializer::new(&mut parser);
        de.deserialize()
    }

    /// Deserialize into a dynamic `Value` using a runtime shape.
    pub fn deserialize_with_shape(
        self,
        source_shape: &'static facet_core::Shape,
    ) -> Result<facet_value::Value, DeserializeError> {
        use facet_format::FormatDeserializer;
        let mut parser = self.parser();
        let mut de = FormatDeserializer::new_owned(&mut parser);
        de.deserialize_with_shape(source_shape)
    }

    /// Deserialize into an existing owned `Partial`.
    pub fn deserialize_into<'facet>(
        self,
        partial: facet_reflect::Partial<'facet, false>,
    ) -> Result<facet_reflect::Partial<'facet, false>, DeserializeError> {
        use facet_format::{FormatDeserializer, MetaSource};
        let mut parser = self.parser();
        let mut de = FormatDeserializer::new_owned(&mut parser);

        #[allow(unsafe_code)]
        let partial: facet_reflect::Partial<'_, false> = unsafe {
            core::mem::transmute::<
                facet_reflect::Partial<'facet, false>,
                facet_reflect::Partial<'_, false>,
            >(partial)
        };

        let partial = de.deserialize_into(partial, MetaSource::FromEvents)?;

        #[allow(unsafe_code)]
        let partial: facet_reflect::Partial<'facet, false> = unsafe {
            core::mem::transmute::<
                facet_reflect::Partial<'_, false>,
                facet_reflect::Partial<'facet, false>,
            >(partial)
        };

        Ok(partial)
    }

    /// Deserialize into an existing borrowed `Partial`.
    pub fn deserialize_into_borrowed<'facet>(
        self,
        partial: facet_reflect::Partial<'facet, true>,
    ) -> Result<facet_reflect::Partial<'facet, true>, DeserializeError>
    where
        'input: 'facet,
    {
        use facet_format::{FormatDeserializer, MetaSource};
        let mut parser = self.parser();
        let mut de = FormatDeserializer::new(&mut parser);
        de.deserialize_into(partial, MetaSource::FromEvents)
    }
}

/// Deserialize a value from postcard bytes into an owned type.
///
/// This is the recommended default for most use cases. The input does not need
/// to outlive the result, making it suitable for deserializing from temporary
/// buffers (e.g., HTTP request bodies).
///
/// Types containing `&str` or `&[u8]` fields cannot be deserialized with this
/// function; use `String`/`Vec<u8>` or `Cow<str>`/`Cow<[u8]>` instead. For
/// zero-copy deserialization into borrowed types, use [`from_slice_borrowed`].
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_postcard::from_slice;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Point {
///     x: i32,
///     y: i32,
/// }
///
/// // Postcard encoding: [x=10 (zigzag), y=20 (zigzag)]
/// let bytes = &[0x14, 0x28];
/// let point: Point = from_slice(bytes).unwrap();
/// assert_eq!(point.x, 10);
/// assert_eq!(point.y, 20);
/// ```
pub fn from_slice<T>(input: &[u8]) -> Result<T, DeserializeError>
where
    T: facet_core::Facet<'static>,
{
    Deserializer::new(input).deserialize()
}

/// Deserialize a value from postcard bytes, allowing zero-copy borrowing.
///
/// This variant requires the input to outlive the result (`'input: 'facet`),
/// enabling zero-copy deserialization of byte slices as `&[u8]` or `Cow<[u8]>`.
///
/// Use this when you need maximum performance and can guarantee the input
/// buffer outlives the deserialized value. For most use cases, prefer
/// [`from_slice`] which doesn't have lifetime requirements.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_postcard::from_slice_borrowed;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Message<'a> {
///     id: u32,
///     data: &'a [u8],
/// }
///
/// // Postcard encoding: [id=1, data_len=3, 0xAB, 0xCD, 0xEF]
/// let bytes = &[0x01, 0x03, 0xAB, 0xCD, 0xEF];
/// let msg: Message = from_slice_borrowed(bytes).unwrap();
/// assert_eq!(msg.id, 1);
/// assert_eq!(msg.data, &[0xAB, 0xCD, 0xEF]);
/// ```
pub fn from_slice_borrowed<'input, 'facet, T>(input: &'input [u8]) -> Result<T, DeserializeError>
where
    T: facet_core::Facet<'facet>,
    'input: 'facet,
{
    Deserializer::new(input).deserialize_borrowed()
}

/// Deserialize postcard bytes into an existing Partial.
///
/// This is useful for reflection-based deserialization where you don't have
/// a concrete type `T` at compile time, only its Shape metadata. The Partial
/// must already be allocated for the target type.
///
/// This version produces owned strings (no borrowing from input).
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_postcard::from_slice_into;
/// use facet_reflect::Partial;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Point {
///     x: i32,
///     y: i32,
/// }
///
/// // Postcard encoding: [x=10 (zigzag), y=20 (zigzag)]
/// let bytes = &[0x14, 0x28];
/// let partial = Partial::alloc_owned::<Point>().unwrap();
/// let partial = from_slice_into(bytes, partial).unwrap();
/// let value = partial.build().unwrap();
/// let point: Point = value.materialize().unwrap();
/// assert_eq!(point.x, 10);
/// assert_eq!(point.y, 20);
/// ```
pub fn from_slice_into<'facet>(
    input: &[u8],
    partial: facet_reflect::Partial<'facet, false>,
) -> Result<facet_reflect::Partial<'facet, false>, DeserializeError> {
    Deserializer::new(input).deserialize_into(partial)
}

/// Deserialize postcard bytes into an existing Partial, allowing zero-copy borrowing.
///
/// This variant requires the input to outlive the Partial's lifetime (`'input: 'facet`),
/// enabling zero-copy deserialization of byte slices as `&[u8]` or `Cow<[u8]>`.
///
/// This is useful for reflection-based deserialization where you don't have
/// a concrete type `T` at compile time, only its Shape metadata.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_postcard::from_slice_into_borrowed;
/// use facet_reflect::Partial;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Message<'a> {
///     id: u32,
///     data: &'a [u8],
/// }
///
/// // Postcard encoding: [id=1, data_len=3, 0xAB, 0xCD, 0xEF]
/// let bytes = &[0x01, 0x03, 0xAB, 0xCD, 0xEF];
/// let partial = Partial::alloc::<Message>().unwrap();
/// let partial = from_slice_into_borrowed(bytes, partial).unwrap();
/// let value = partial.build().unwrap();
/// let msg: Message = value.materialize().unwrap();
/// assert_eq!(msg.id, 1);
/// assert_eq!(msg.data, &[0xAB, 0xCD, 0xEF]);
/// ```
pub fn from_slice_into_borrowed<'input, 'facet>(
    input: &'input [u8],
    partial: facet_reflect::Partial<'facet, true>,
) -> Result<facet_reflect::Partial<'facet, true>, DeserializeError>
where
    'input: 'facet,
{
    Deserializer::new(input).deserialize_into_borrowed(partial)
}
