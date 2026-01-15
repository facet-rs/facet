//! Serialize any type implementing `Facet` into a `Value`.
//!
//! This module provides the inverse of [`from_value`](crate::from_value): given a Rust type
//! that implements `Facet`, you can serialize it into a dynamic `Value`.
//!
//! # Example
//!
//! ```
//! use facet::Facet;
//! use facet_value::{Value, to_value, from_value};
//!
//! #[derive(Debug, Facet, PartialEq)]
//! struct Person {
//!     name: String,
//!     age: u32,
//! }
//!
//! let person = Person { name: "Alice".into(), age: 30 };
//!
//! // Serialize to a Value
//! let value = to_value(&person).unwrap();
//!
//! // The value can be inspected dynamically
//! assert!(value.is_object());
//! let obj = value.as_object().unwrap();
//! assert_eq!(obj.get("name").unwrap().as_string().unwrap().as_str(), "Alice");
//! assert_eq!(obj.get("age").unwrap().as_number().unwrap().to_u64(), Some(30));
//!
//! // Or deserialized back to the original type
//! let person2: Person = from_value(value).unwrap();
//! assert_eq!(person, person2);
//! ```

use alloc::string::String;
use alloc::vec::Vec;

use facet_core::Facet;
use facet_format::{FormatSerializer, ScalarValue, SerializeError, serialize_root};
use facet_reflect::Peek;

use crate::{VArray, VNumber, VObject, VString, Value};

/// Error type for Value serialization.
#[derive(Debug)]
pub struct ToValueError {
    msg: String,
}

impl ToValueError {
    /// Create a new error with the given message.
    pub fn new(msg: impl Into<String>) -> Self {
        Self { msg: msg.into() }
    }
}

impl core::fmt::Display for ToValueError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.msg)
    }
}

#[cfg(feature = "std")]
impl core::error::Error for ToValueError {}

/// Serializer that builds a `Value` from a sequence of format events.
struct ValueSerializer {
    /// Stack of containers being built
    stack: Vec<StackFrame>,
    /// The final result
    result: Option<Value>,
}

/// A frame on the serialization stack, representing a container being built.
enum StackFrame {
    /// Building an object
    Object {
        obj: VObject,
        pending_key: Option<String>,
    },
    /// Building an array
    Array { arr: VArray },
}

impl ValueSerializer {
    fn new() -> Self {
        Self {
            stack: Vec::new(),
            result: None,
        }
    }

    fn finish(self) -> Value {
        self.result.unwrap_or(Value::NULL)
    }

    fn emit(&mut self, value: Value) {
        match self.stack.last_mut() {
            Some(StackFrame::Object { obj, pending_key }) => {
                if let Some(key) = pending_key.take() {
                    obj.insert(key, value);
                } else {
                    // This shouldn't happen in well-formed input
                    panic!("emit called on object without pending key");
                }
            }
            Some(StackFrame::Array { arr }) => {
                arr.push(value);
            }
            None => {
                self.result = Some(value);
            }
        }
    }
}

impl FormatSerializer for ValueSerializer {
    type Error = ToValueError;

    fn begin_struct(&mut self) -> Result<(), Self::Error> {
        self.stack.push(StackFrame::Object {
            obj: VObject::new(),
            pending_key: None,
        });
        Ok(())
    }

    fn field_key(&mut self, key: &str) -> Result<(), Self::Error> {
        match self.stack.last_mut() {
            Some(StackFrame::Object { pending_key, .. }) => {
                *pending_key = Some(key.to_string());
                Ok(())
            }
            _ => Err(ToValueError::new("field_key called outside of object")),
        }
    }

    fn end_struct(&mut self) -> Result<(), Self::Error> {
        match self.stack.pop() {
            Some(StackFrame::Object { obj, .. }) => {
                self.emit(obj.into());
                Ok(())
            }
            _ => Err(ToValueError::new(
                "end_struct called without matching begin_struct",
            )),
        }
    }

    fn begin_seq(&mut self) -> Result<(), Self::Error> {
        self.stack.push(StackFrame::Array { arr: VArray::new() });
        Ok(())
    }

    fn end_seq(&mut self) -> Result<(), Self::Error> {
        match self.stack.pop() {
            Some(StackFrame::Array { arr }) => {
                self.emit(arr.into());
                Ok(())
            }
            _ => Err(ToValueError::new(
                "end_seq called without matching begin_seq",
            )),
        }
    }

    fn scalar(&mut self, scalar: ScalarValue<'_>) -> Result<(), Self::Error> {
        let value = match scalar {
            ScalarValue::Unit | ScalarValue::Null => Value::NULL,
            ScalarValue::Bool(b) => Value::from(b),
            ScalarValue::Char(c) => VString::new(&c.to_string()).into(),
            ScalarValue::I64(n) => VNumber::from_i64(n).into(),
            ScalarValue::U64(n) => VNumber::from_u64(n).into(),
            ScalarValue::I128(n) => {
                // Store as string for large numbers
                VString::new(&n.to_string()).into()
            }
            ScalarValue::U128(n) => {
                // Store as string for large numbers
                VString::new(&n.to_string()).into()
            }
            ScalarValue::F64(n) => VNumber::from_f64(n).map(Into::into).unwrap_or(Value::NULL),
            ScalarValue::Str(s) => VString::new(&s).into(),
            ScalarValue::Bytes(b) => crate::VBytes::new(b.as_ref()).into(),
        };
        self.emit(value);
        Ok(())
    }
}

/// Serialize a value implementing `Facet` into a `Value`.
///
/// This is the main entry point for converting a typed Rust value into a
/// dynamic `Value` that can be inspected, modified, or serialized to various formats.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_value::to_value;
///
/// #[derive(Facet)]
/// struct Point {
///     x: i32,
///     y: i32,
/// }
///
/// let point = Point { x: 10, y: 20 };
/// let value = to_value(&point).unwrap();
///
/// // Access fields dynamically
/// let obj = value.as_object().unwrap();
/// assert_eq!(obj.get("x").unwrap().as_number().unwrap().to_i64(), Some(10));
/// assert_eq!(obj.get("y").unwrap().as_number().unwrap().to_i64(), Some(20));
/// ```
pub fn to_value<'facet, T: Facet<'facet>>(
    value: &T,
) -> Result<Value, SerializeError<ToValueError>> {
    let mut serializer = ValueSerializer::new();
    serialize_root(&mut serializer, Peek::new(value))?;
    Ok(serializer.finish())
}

/// Serialize a `Peek` instance into a `Value`.
///
/// This allows serializing values without requiring ownership, useful when
/// you already have a `Peek` from reflection operations.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_reflect::Peek;
/// use facet_value::peek_to_value;
///
/// #[derive(Facet)]
/// struct Point { x: i32, y: i32 }
///
/// let point = Point { x: 10, y: 20 };
/// let value = peek_to_value(Peek::new(&point)).unwrap();
///
/// assert!(value.is_object());
/// ```
pub fn peek_to_value<'mem, 'facet>(
    peek: Peek<'mem, 'facet>,
) -> Result<Value, SerializeError<ToValueError>> {
    let mut serializer = ValueSerializer::new();
    serialize_root(&mut serializer, peek)?;
    Ok(serializer.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::BTreeMap;
    use alloc::string::ToString;
    use alloc::vec;

    #[test]
    fn test_to_value_primitives() {
        // bool
        let v = to_value(&true).unwrap();
        assert_eq!(v.as_bool(), Some(true));

        let v = to_value(&false).unwrap();
        assert_eq!(v.as_bool(), Some(false));

        // integers
        let v = to_value(&42i32).unwrap();
        assert_eq!(v.as_number().unwrap().to_i64(), Some(42));

        let v = to_value(&123u64).unwrap();
        assert_eq!(v.as_number().unwrap().to_u64(), Some(123));

        // float
        let v = to_value(&2.5f64).unwrap();
        assert!((v.as_number().unwrap().to_f64().unwrap() - 2.5).abs() < 0.001);

        // string
        let s = "hello".to_string();
        let v = to_value(&s).unwrap();
        assert_eq!(v.as_string().unwrap().as_str(), "hello");
    }

    #[test]
    fn test_to_value_option() {
        let some: Option<i32> = Some(42);
        let v = to_value(&some).unwrap();
        assert_eq!(v.as_number().unwrap().to_i64(), Some(42));

        let none: Option<i32> = None;
        let v = to_value(&none).unwrap();
        assert!(v.is_null());
    }

    #[test]
    fn test_to_value_vec() {
        let vec = vec![1i32, 2, 3];
        let v = to_value(&vec).unwrap();

        let arr = v.as_array().unwrap();
        assert_eq!(arr.len(), 3);
        assert_eq!(arr.get(0).unwrap().as_number().unwrap().to_i64(), Some(1));
        assert_eq!(arr.get(1).unwrap().as_number().unwrap().to_i64(), Some(2));
        assert_eq!(arr.get(2).unwrap().as_number().unwrap().to_i64(), Some(3));
    }

    #[test]
    fn test_to_value_map() {
        let mut map: BTreeMap<String, i32> = BTreeMap::new();
        map.insert("a".to_string(), 1);
        map.insert("b".to_string(), 2);

        let v = to_value(&map).unwrap();

        let obj = v.as_object().unwrap();
        assert_eq!(obj.get("a").unwrap().as_number().unwrap().to_i64(), Some(1));
        assert_eq!(obj.get("b").unwrap().as_number().unwrap().to_i64(), Some(2));
    }

    #[test]
    fn test_to_value_nested() {
        // Vec<Option<i32>>
        let vec = vec![Some(1i32), None, Some(3)];
        let v = to_value(&vec).unwrap();

        let arr = v.as_array().unwrap();
        assert_eq!(arr.len(), 3);
        assert_eq!(arr.get(0).unwrap().as_number().unwrap().to_i64(), Some(1));
        assert!(arr.get(1).unwrap().is_null());
        assert_eq!(arr.get(2).unwrap().as_number().unwrap().to_i64(), Some(3));
    }

    #[test]
    fn test_roundtrip_value() {
        // Roundtrip: Value -> Value should be identity
        let original = crate::value!({
            "name": "Alice",
            "age": 30,
            "active": true
        });

        let v = to_value(&original).unwrap();
        assert_eq!(v, original);
    }
}
