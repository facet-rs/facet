// Note: unsafe code is used for lifetime transmutes in from_slice_into/from_str_into
// when BORROW=false, mirroring the approach used in facet-format's FormatDeserializer.

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
use facet_reflect::Partial;
#[allow(unused_imports)]
pub(crate) use trace;

mod error;
mod parser;
mod raw_json;
mod scanner;
mod serializer;

#[cfg(feature = "jit")]
pub mod jit;

#[cfg(feature = "axum")]
mod axum;

#[cfg(feature = "jit")]
pub use jit::JsonJitFormat;

#[cfg(feature = "axum")]
pub use axum::{Json, JsonRejection};

pub use error::JsonError;
pub use parser::JsonParser;
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
pub fn from_str<T>(input: &str) -> Result<T, DeserializeError>
where
    T: facet_core::Facet<'static>,
{
    use facet_format::FormatDeserializer;
    // TRUSTED_UTF8 = true: input came from &str, so it's valid UTF-8
    let mut parser = JsonParser::<true>::new(input.as_bytes());
    let mut de = FormatDeserializer::new_owned(&mut parser);
    de.deserialize_root()
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
pub fn from_slice<T>(input: &[u8]) -> Result<T, DeserializeError>
where
    T: facet_core::Facet<'static>,
{
    use facet_format::FormatDeserializer;
    let mut parser = JsonParser::<false>::new(input);
    let mut de = FormatDeserializer::new_owned(&mut parser);
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
pub fn from_str_borrowed<'input, 'facet, T>(input: &'input str) -> Result<T, DeserializeError>
where
    T: facet_core::Facet<'facet>,
    'input: 'facet,
{
    use facet_format::FormatDeserializer;
    // TRUSTED_UTF8 = true: input came from &str, so it's valid UTF-8
    let mut parser = JsonParser::<true>::new(input.as_bytes());
    let mut de = FormatDeserializer::new(&mut parser);
    de.deserialize_root()
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
pub fn from_slice_borrowed<'input, 'facet, T>(input: &'input [u8]) -> Result<T, DeserializeError>
where
    T: facet_core::Facet<'facet>,
    'input: 'facet,
{
    use facet_format::FormatDeserializer;
    let mut parser = JsonParser::<false>::new(input);
    let mut de = FormatDeserializer::new(&mut parser);
    de.deserialize_root()
}

/// Deserialize JSON from a string into an existing Partial.
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
/// use facet_json::from_str_into;
/// use facet_reflect::Partial;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Person {
///     name: String,
///     age: u32,
/// }
///
/// let json = r#"{"name": "Alice", "age": 30}"#;
/// let partial = Partial::alloc_owned::<Person>().unwrap();
/// let partial = from_str_into(json, partial).unwrap();
/// let value = partial.build().unwrap();
/// let person: Person = value.materialize().unwrap();
/// assert_eq!(person.name, "Alice");
/// assert_eq!(person.age, 30);
/// ```
pub fn from_str_into<'facet>(
    input: &str,
    partial: Partial<'facet, false>,
) -> Result<Partial<'facet, false>, DeserializeError> {
    use facet_format::FormatDeserializer;
    // TRUSTED_UTF8 = true: input came from &str, so it's valid UTF-8
    let mut parser = JsonParser::<true>::new(input.as_bytes());
    let mut de = FormatDeserializer::new_owned(&mut parser);

    // SAFETY: The deserializer expects Partial<'input, false> where 'input is the
    // lifetime of the JSON bytes. Since BORROW=false, no data is borrowed from the
    // input, so the actual 'facet lifetime of the Partial is independent of 'input.
    // We transmute to satisfy the type system, then transmute back after deserialization.
    #[allow(unsafe_code)]
    let partial: Partial<'_, false> =
        unsafe { core::mem::transmute::<Partial<'facet, false>, Partial<'_, false>>(partial) };

    let partial = de.deserialize_into(partial)?;

    // SAFETY: Same reasoning - no borrowed data since BORROW=false.
    #[allow(unsafe_code)]
    let partial: Partial<'facet, false> =
        unsafe { core::mem::transmute::<Partial<'_, false>, Partial<'facet, false>>(partial) };

    Ok(partial)
}

/// Deserialize JSON from bytes into an existing Partial.
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
/// use facet_json::from_slice_into;
/// use facet_reflect::Partial;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Point {
///     x: i32,
///     y: i32,
/// }
///
/// let json = br#"{"x": 10, "y": 20}"#;
/// let partial = Partial::alloc_owned::<Point>().unwrap();
/// let partial = from_slice_into(json, partial).unwrap();
/// let value = partial.build().unwrap();
/// let point: Point = value.materialize().unwrap();
/// assert_eq!(point.x, 10);
/// assert_eq!(point.y, 20);
/// ```
pub fn from_slice_into<'facet>(
    input: &[u8],
    partial: Partial<'facet, false>,
) -> Result<Partial<'facet, false>, DeserializeError> {
    use facet_format::FormatDeserializer;
    let mut parser = JsonParser::<false>::new(input);
    let mut de = FormatDeserializer::new_owned(&mut parser);

    // SAFETY: The deserializer expects Partial<'input, false> where 'input is the
    // lifetime of the JSON bytes. Since BORROW=false, no data is borrowed from the
    // input, so the actual 'facet lifetime of the Partial is independent of 'input.
    // We transmute to satisfy the type system, then transmute back after deserialization.
    #[allow(unsafe_code)]
    let partial: Partial<'_, false> =
        unsafe { core::mem::transmute::<Partial<'facet, false>, Partial<'_, false>>(partial) };

    let partial = de.deserialize_into(partial)?;

    // SAFETY: Same reasoning - no borrowed data since BORROW=false.
    #[allow(unsafe_code)]
    let partial: Partial<'facet, false> =
        unsafe { core::mem::transmute::<Partial<'_, false>, Partial<'facet, false>>(partial) };

    Ok(partial)
}

/// Deserialize JSON from a string into an existing Partial, allowing zero-copy borrowing.
///
/// This variant requires the input to outlive the Partial's lifetime (`'input: 'facet`),
/// enabling zero-copy deserialization of string fields as `&str` or `Cow<str>`.
///
/// This is useful for reflection-based deserialization where you don't have
/// a concrete type `T` at compile time, only its Shape metadata.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_json::from_str_into_borrowed;
/// use facet_reflect::Partial;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Person<'a> {
///     name: &'a str,
///     age: u32,
/// }
///
/// let json = r#"{"name": "Alice", "age": 30}"#;
/// let partial = Partial::alloc::<Person>().unwrap();
/// let partial = from_str_into_borrowed(json, partial).unwrap();
/// let value = partial.build().unwrap();
/// let person: Person = value.materialize().unwrap();
/// assert_eq!(person.name, "Alice");
/// assert_eq!(person.age, 30);
/// ```
pub fn from_str_into_borrowed<'input, 'facet>(
    input: &'input str,
    partial: Partial<'facet, true>,
) -> Result<Partial<'facet, true>, DeserializeError>
where
    'input: 'facet,
{
    use facet_format::FormatDeserializer;
    // TRUSTED_UTF8 = true: input came from &str, so it's valid UTF-8
    let mut parser = JsonParser::<true>::new(input.as_bytes());
    let mut de = FormatDeserializer::new(&mut parser);
    de.deserialize_into(partial)
}

/// Deserialize JSON from bytes into an existing Partial, allowing zero-copy borrowing.
///
/// This variant requires the input to outlive the Partial's lifetime (`'input: 'facet`),
/// enabling zero-copy deserialization of string fields as `&str` or `Cow<str>`.
///
/// This is useful for reflection-based deserialization where you don't have
/// a concrete type `T` at compile time, only its Shape metadata.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_json::from_slice_into_borrowed;
/// use facet_reflect::Partial;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Point<'a> {
///     label: &'a str,
///     x: i32,
///     y: i32,
/// }
///
/// let json = br#"{"label": "origin", "x": 0, "y": 0}"#;
/// let partial = Partial::alloc::<Point>().unwrap();
/// let partial = from_slice_into_borrowed(json, partial).unwrap();
/// let value = partial.build().unwrap();
/// let point: Point = value.materialize().unwrap();
/// assert_eq!(point.label, "origin");
/// ```
pub fn from_slice_into_borrowed<'input, 'facet>(
    input: &'input [u8],
    partial: Partial<'facet, true>,
) -> Result<Partial<'facet, true>, DeserializeError>
where
    'input: 'facet,
{
    use facet_format::FormatDeserializer;
    let mut parser = JsonParser::<false>::new(input);
    let mut de = FormatDeserializer::new(&mut parser);
    de.deserialize_into(partial)
}
