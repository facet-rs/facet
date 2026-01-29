//! Shape-based deserialization into `facet_value::Value`.
//!
//! This module provides deserialization from postcard bytes into a `Value` using only
//! a `&'static Shape` at runtime, without requiring a concrete Rust type.

use bumpalo::Bump;
use facet_core::Shape;
use facet_format::{DeserializeError, FormatDeserializer};
use facet_value::Value;

use crate::parser::PostcardParser;

/// Deserialize postcard bytes into a `Value` using shape information.
///
/// Since postcard is not a self-describing format, you need to provide the shape
/// that describes the structure of the data.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_postcard::{from_slice_with_shape, to_vec};
///
/// #[derive(Facet)]
/// struct Point { x: i32, y: i32 }
///
/// let point = Point { x: 10, y: 20 };
/// let bytes = to_vec(&point).unwrap();
///
/// // Deserialize using the shape, not the type
/// let value = from_slice_with_shape(&bytes, Point::SHAPE).unwrap();
/// let obj = value.as_object().unwrap();
/// assert_eq!(obj.get("x").unwrap().as_number().unwrap().to_i64(), Some(10));
/// assert_eq!(obj.get("y").unwrap().as_number().unwrap().to_i64(), Some(20));
/// ```
pub fn from_slice_with_shape(
    input: &[u8],
    source_shape: &'static Shape,
) -> Result<Value, DeserializeError> {
    let bump = Bump::new();
    let mut parser = PostcardParser::new(input);
    let mut de = FormatDeserializer::new_owned(&bump, &mut parser);
    de.deserialize_with_shape(source_shape)
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;

    #[test]
    fn test_from_slice_with_shape_primitives() {
        // Test i32
        let bytes = crate::to_vec(&42i32).unwrap();
        let value = from_slice_with_shape(&bytes, i32::SHAPE).unwrap();
        assert_eq!(value.as_number().unwrap().to_i64(), Some(42));

        // Test String
        let bytes = crate::to_vec(&"hello".to_string()).unwrap();
        let value = from_slice_with_shape(&bytes, String::SHAPE).unwrap();
        assert_eq!(value.as_string().unwrap().as_str(), "hello");

        // Test bool
        let bytes = crate::to_vec(&true).unwrap();
        let value = from_slice_with_shape(&bytes, bool::SHAPE).unwrap();
        assert_eq!(value.as_bool(), Some(true));
    }

    #[test]
    fn test_from_slice_with_shape_struct() {
        #[derive(Facet)]
        struct Point {
            x: i32,
            y: i32,
        }

        let point = Point { x: 10, y: 20 };
        let bytes = crate::to_vec(&point).unwrap();
        let value = from_slice_with_shape(&bytes, Point::SHAPE).unwrap();

        let obj = value.as_object().unwrap();
        assert_eq!(
            obj.get("x").unwrap().as_number().unwrap().to_i64(),
            Some(10)
        );
        assert_eq!(
            obj.get("y").unwrap().as_number().unwrap().to_i64(),
            Some(20)
        );
    }

    #[test]
    fn test_from_slice_with_shape_vec() {
        let vec = vec![1i32, 2, 3, 4, 5];
        let bytes = crate::to_vec(&vec).unwrap();
        let value = from_slice_with_shape(&bytes, <Vec<i32>>::SHAPE).unwrap();

        let arr = value.as_array().unwrap();
        assert_eq!(arr.len(), 5);
        assert_eq!(arr.get(0).unwrap().as_number().unwrap().to_i64(), Some(1));
        assert_eq!(arr.get(4).unwrap().as_number().unwrap().to_i64(), Some(5));
    }

    #[test]
    fn test_from_slice_with_shape_option() {
        // Some value
        let opt: Option<i32> = Some(42);
        let bytes = crate::to_vec(&opt).unwrap();
        let value = from_slice_with_shape(&bytes, <Option<i32>>::SHAPE).unwrap();
        assert_eq!(value.as_number().unwrap().to_i64(), Some(42));

        // None
        let opt: Option<i32> = None;
        let bytes = crate::to_vec(&opt).unwrap();
        let value = from_slice_with_shape(&bytes, <Option<i32>>::SHAPE).unwrap();
        assert!(value.is_null());
    }

    #[test]
    fn test_from_slice_with_shape_enum() {
        #[derive(Facet)]
        #[repr(u8)]
        #[allow(dead_code)]
        enum Message {
            Ping,
            Text(String),
            Point { x: i32, y: i32 },
        }

        // Unit variant
        let msg = Message::Ping;
        let bytes = crate::to_vec(&msg).unwrap();
        let value = from_slice_with_shape(&bytes, Message::SHAPE).unwrap();
        assert_eq!(value.as_string().unwrap().as_str(), "Ping");

        // Newtype variant
        let msg = Message::Text("hello".into());
        let bytes = crate::to_vec(&msg).unwrap();
        let value = from_slice_with_shape(&bytes, Message::SHAPE).unwrap();
        let obj = value.as_object().unwrap();
        assert_eq!(
            obj.get("Text").unwrap().as_string().unwrap().as_str(),
            "hello"
        );

        // Struct variant
        let msg = Message::Point { x: 10, y: 20 };
        let bytes = crate::to_vec(&msg).unwrap();
        let value = from_slice_with_shape(&bytes, Message::SHAPE).unwrap();
        let obj = value.as_object().unwrap();
        let inner = obj.get("Point").unwrap().as_object().unwrap();
        assert_eq!(
            inner.get("x").unwrap().as_number().unwrap().to_i64(),
            Some(10)
        );
        assert_eq!(
            inner.get("y").unwrap().as_number().unwrap().to_i64(),
            Some(20)
        );
    }
}
