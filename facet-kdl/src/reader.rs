//! Reader support for deserializing KDL from I/O streams.

use crate::{KdlDeserializeError, from_str};
use facet_core::Facet;

/// Deserialize a value from a reader.
///
/// Reads the entire input into memory before parsing.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_kdl::from_reader;
/// use std::io::Cursor;
///
/// #[derive(Facet, Debug)]
/// struct Config {
///     host: String,
///     port: u16,
/// }
///
/// let kdl = b"host \"localhost\"\nport 8080";
/// let reader = Cursor::new(kdl);
/// let config: Config = from_reader(reader).unwrap();
/// ```
#[allow(clippy::result_large_err)]
pub fn from_reader<R, T>(mut reader: R) -> Result<T, KdlDeserializeError>
where
    R: std::io::Read,
    T: Facet<'static>,
{
    let mut buf = String::new();
    reader.read_to_string(&mut buf).map_err(|e| {
        let inner =
            crate::DeserializeError::Parser(crate::KdlError::InvalidStructure(e.to_string()));
        KdlDeserializeError::new(inner, String::new(), Some(T::SHAPE))
    })?;
    from_str(&buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;
    use std::io::Cursor;

    #[test]
    fn test_from_reader() {
        #[derive(Facet, Debug, PartialEq)]
        struct Config {
            host: String,
            port: u16,
        }

        let kdl = b"host \"localhost\"\nport 8080";
        let reader = Cursor::new(&kdl[..]);
        let config: Config = from_reader(reader).unwrap();

        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, 8080);
    }
}
