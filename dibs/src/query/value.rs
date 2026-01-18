//! Runtime values for query parameters.

use facet::Facet;

/// A runtime SQL value.
///
/// Used for query parameters and row data. Maps to Postgres types.
#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
pub enum Value {
    /// NULL
    Null,

    /// Boolean
    Bool(bool),

    /// 16-bit signed integer (SMALLINT)
    I16(i16),

    /// 32-bit signed integer (INTEGER)
    I32(i32),

    /// 64-bit signed integer (BIGINT)
    I64(i64),

    /// 32-bit float (REAL)
    F32(f32),

    /// 64-bit float (DOUBLE PRECISION)
    F64(f64),

    /// Text (TEXT, VARCHAR, etc.)
    String(String),

    /// Binary data (BYTEA)
    Bytes(Vec<u8>),
    // TODO: Add as needed:
    // Timestamp(jiff::Timestamp),
    // Uuid(uuid::Uuid),
    // Json(serde_json::Value),
    // Array(Vec<Value>),
}

impl Value {
    /// Returns true if this is a NULL value.
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }
}

// Convenient From impls
impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Value::Bool(v)
    }
}

impl From<i16> for Value {
    fn from(v: i16) -> Self {
        Value::I16(v)
    }
}

impl From<i32> for Value {
    fn from(v: i32) -> Self {
        Value::I32(v)
    }
}

impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Value::I64(v)
    }
}

impl From<f32> for Value {
    fn from(v: f32) -> Self {
        Value::F32(v)
    }
}

impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Value::F64(v)
    }
}

impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::String(v)
    }
}

impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::String(v.to_owned())
    }
}

impl From<Vec<u8>> for Value {
    fn from(v: Vec<u8>) -> Self {
        Value::Bytes(v)
    }
}

impl<T: Into<Value>> From<Option<T>> for Value {
    fn from(v: Option<T>) -> Self {
        match v {
            Some(v) => v.into(),
            None => Value::Null,
        }
    }
}
