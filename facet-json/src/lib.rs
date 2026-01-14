#![cfg_attr(not(feature = "jit"), forbid(unsafe_code))]

//! JSON parser and serializer using facet-format.
//!
//! This crate provides JSON support via the `FormatParser` trait.

extern crate alloc;

/// Trace-level logging macro that forwards to `tracing::trace!` when the `tracing` feature is enabled.
#[cfg(feature = "tracing")]
#[allow(unused_macros)]
macro_rules! trace {
    ($($arg:tt)*) => {
        ::tracing::trace!($($arg)*)
    };
}

/// Trace-level logging macro (no-op when `tracing` feature is disabled).
#[cfg(not(feature = "tracing"))]
#[allow(unused_macros)]
macro_rules! trace {
    ($($arg:tt)*) => {};
}

/// Debug-level logging macro that forwards to `tracing::debug!` when the `tracing` feature is enabled.
#[cfg(feature = "tracing")]
#[allow(unused_macros)]
macro_rules! debug {
    ($($arg:tt)*) => {
        ::tracing::debug!($($arg)*)
    };
}

/// Debug-level logging macro (no-op when `tracing` feature is disabled).
#[cfg(not(feature = "tracing"))]
#[allow(unused_macros)]
macro_rules! debug {
    ($($arg:tt)*) => {};
}

#[allow(unused_imports)]
pub(crate) use debug;
#[allow(unused_imports)]
pub(crate) use trace;

mod adapter;
mod error;
mod parser;
mod raw_json;
#[cfg(feature = "streaming")]
mod scan_buffer;
mod scanner;
mod serializer;

#[cfg(feature = "streaming")]
mod streaming_adapter;

#[cfg(feature = "jit")]
pub mod jit;

#[cfg(feature = "axum")]
mod axum;

#[cfg(feature = "jit")]
pub use jit::JsonJitFormat;

#[cfg(feature = "axum")]
pub use axum::{Json, JsonRejection};
pub use parser::{JsonError, JsonParser};
pub use raw_json::RawJson;
pub use serializer::{
    JsonSerializeError, JsonSerializer, SerializeOptions, peek_to_string, peek_to_string_pretty,
    peek_to_string_with_options, peek_to_writer_std, peek_to_writer_std_pretty,
    peek_to_writer_std_with_options, to_string, to_string_pretty, to_string_with_options, to_vec,
    to_vec_pretty, to_vec_with_options, to_writer_std, to_writer_std_pretty,
    to_writer_std_with_options,
};

// Re-export DeserializeError for convenience
pub use facet_format::DeserializeError;

/// Deserialize a value from a JSON string into an owned type.
///
/// This is the recommended default for most use cases. The input does not need
/// to outlive the result, making it suitable for deserializing from temporary
/// buffers (e.g., HTTP request bodies).
///
/// Types containing `&str` fields cannot be deserialized with this function;
/// use `String` or `Cow<str>` instead. For zero-copy deserialization into
/// borrowed types, use [`from_str_borrowed`].
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_json::from_str;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Person {
///     name: String,
///     age: u32,
/// }
///
/// let json = r#"{"name": "Alice", "age": 30}"#;
/// let person: Person = from_str(json).unwrap();
/// assert_eq!(person.name, "Alice");
/// assert_eq!(person.age, 30);
/// ```
pub fn from_str<T>(input: &str) -> Result<T, DeserializeError<JsonError>>
where
    T: facet_core::Facet<'static>,
{
    from_slice(input.as_bytes())
}

/// Deserialize a value from JSON bytes into an owned type.
///
/// This is the recommended default for most use cases. The input does not need
/// to outlive the result, making it suitable for deserializing from temporary
/// buffers (e.g., HTTP request bodies).
///
/// Types containing `&str` fields cannot be deserialized with this function;
/// use `String` or `Cow<str>` instead. For zero-copy deserialization into
/// borrowed types, use [`from_slice_borrowed`].
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_json::from_slice;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Point {
///     x: i32,
///     y: i32,
/// }
///
/// let json = br#"{"x": 10, "y": 20}"#;
/// let point: Point = from_slice(json).unwrap();
/// assert_eq!(point.x, 10);
/// assert_eq!(point.y, 20);
/// ```
pub fn from_slice<T>(input: &[u8]) -> Result<T, DeserializeError<JsonError>>
where
    T: facet_core::Facet<'static>,
{
    use facet_format::FormatDeserializer;
    let parser = JsonParser::new(input);
    let mut de = FormatDeserializer::new_owned(parser);
    de.deserialize_root()
}

/// Deserialize a value from a JSON string, allowing zero-copy borrowing.
///
/// This variant requires the input to outlive the result (`'input: 'facet`),
/// enabling zero-copy deserialization of string fields as `&str` or `Cow<str>`.
///
/// Use this when you need maximum performance and can guarantee the input
/// buffer outlives the deserialized value. For most use cases, prefer
/// [`from_str`] which doesn't have lifetime requirements.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_json::from_str_borrowed;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Person<'a> {
///     name: &'a str,
///     age: u32,
/// }
///
/// let json = r#"{"name": "Alice", "age": 30}"#;
/// let person: Person = from_str_borrowed(json).unwrap();
/// assert_eq!(person.name, "Alice");
/// assert_eq!(person.age, 30);
/// ```
pub fn from_str_borrowed<'input, 'facet, T>(
    input: &'input str,
) -> Result<T, DeserializeError<JsonError>>
where
    T: facet_core::Facet<'facet>,
    'input: 'facet,
{
    from_slice_borrowed(input.as_bytes())
}

/// Deserialize a value from JSON bytes, allowing zero-copy borrowing.
///
/// This variant requires the input to outlive the result (`'input: 'facet`),
/// enabling zero-copy deserialization of string fields as `&str` or `Cow<str>`.
///
/// Use this when you need maximum performance and can guarantee the input
/// buffer outlives the deserialized value. For most use cases, prefer
/// [`from_slice`] which doesn't have lifetime requirements.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_json::from_slice_borrowed;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Point<'a> {
///     label: &'a str,
///     x: i32,
///     y: i32,
/// }
///
/// let json = br#"{"label": "origin", "x": 0, "y": 0}"#;
/// let point: Point = from_slice_borrowed(json).unwrap();
/// assert_eq!(point.label, "origin");
/// ```
pub fn from_slice_borrowed<'input, 'facet, T>(
    input: &'input [u8],
) -> Result<T, DeserializeError<JsonError>>
where
    T: facet_core::Facet<'facet>,
    'input: 'facet,
{
    use facet_format::FormatDeserializer;
    let parser = JsonParser::new(input);
    let mut de = FormatDeserializer::new(parser);
    de.deserialize_root()
}

#[cfg(feature = "streaming")]
mod streaming;
#[cfg(feature = "futures-io")]
pub use streaming::from_async_reader_futures;
#[cfg(feature = "tokio")]
pub use streaming::from_async_reader_tokio;
#[cfg(feature = "streaming")]
pub use streaming::from_reader;
