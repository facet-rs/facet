//! Async XML deserialization support.
//!
//! This module provides async deserialization by buffering the entire input
//! from an async reader and then using the synchronous parser.
//!
//! Note: Unlike facet-format-json's true streaming, this buffers all input first
//! because XML parsing requires building a complete DOM tree before emitting events.

use facet_core::Facet;
use facet_format::{DeserializeError, FormatDeserializer};

use crate::{XmlError, XmlParser};

/// Deserialize XML from an async reader (tokio).
///
/// This function buffers the entire input asynchronously, then parses it.
///
/// # Example
///
/// ```ignore
/// use std::io::Cursor;
/// use facet::Facet;
/// use facet_format_xml::from_async_reader_tokio;
///
/// #[derive(Facet, Debug)]
/// struct Person {
///     name: String,
///     age: u32,
/// }
///
/// let xml = b"<person><name>Alice</name><age>30</age></person>";
/// let reader = Cursor::new(xml);
/// let person: Person = from_async_reader_tokio(reader).await.unwrap();
/// ```
#[cfg(feature = "tokio")]
pub async fn from_async_reader_tokio<R, T>(mut reader: R) -> Result<T, DeserializeError<XmlError>>
where
    R: tokio::io::AsyncRead + Unpin,
    T: Facet<'static>,
{
    use tokio::io::AsyncReadExt;

    // Buffer all input
    let mut buffer = Vec::new();
    reader
        .read_to_end(&mut buffer)
        .await
        .map_err(|e| DeserializeError::Parser(XmlError::ParseError(format!("IO error: {}", e))))?;

    // Use the sync parser
    let parser = XmlParser::new(&buffer);
    let mut de = FormatDeserializer::new_owned(parser);
    de.deserialize_root::<T>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;
    use std::io::Cursor;

    #[tokio::test]
    async fn test_from_async_reader_simple() {
        #[derive(Facet, Debug, PartialEq)]
        struct Person {
            name: String,
            age: u32,
        }

        let xml = b"<person><name>Alice</name><age>30</age></person>";
        let reader = Cursor::new(&xml[..]);
        let person: Person = from_async_reader_tokio(reader).await.unwrap();

        assert_eq!(person.name, "Alice");
        assert_eq!(person.age, 30);
    }
}
