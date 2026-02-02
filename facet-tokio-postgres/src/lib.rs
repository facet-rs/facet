//! Deserialize tokio-postgres Rows into any type implementing Facet.
//!
//! This crate provides a bridge between tokio-postgres and facet, allowing you to
//! deserialize database rows directly into Rust structs that implement `Facet`.
//!
//! # Example
//!
//! ```ignore
//! use facet::Facet;
//! use facet_tokio_postgres::from_row;
//!
//! #[derive(Debug, Facet)]
//! struct User {
//!     id: i32,
//!     name: String,
//!     email: Option<String>,
//! }
//!
//! // After executing a query...
//! let row = client.query_one("SELECT id, name, email FROM users WHERE id = $1", &[&1]).await?;
//! let user: User = from_row(&row)?;
//! ```

mod jsonb;
use jsonb::{OptionalRawJsonb, RawJsonb};

pub use dibs_jsonb::Jsonb;

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use facet_core::{Facet, Shape, StructKind, Type, UserType};
use facet_reflect::{AllocError, Partial, ReflectError, ShapeMismatchError};
use tokio_postgres::Row;

/// Error type for Row deserialization.
#[derive(Debug)]
pub enum Error {
    /// A required column was not found in the row
    MissingColumn {
        /// Name of the missing column
        column: String,
    },
    /// The column type doesn't match the expected Rust type
    TypeMismatch {
        /// Name of the column
        column: String,
        /// Expected type
        expected: &'static Shape,
        /// Actual error from postgres
        source: tokio_postgres::Error,
    },
    /// Error from facet reflection
    Reflect(ReflectError),
    /// Error allocating memory for reflection
    Alloc(AllocError),
    /// Shape mismatch error during materialization
    ShapeMismatch(ShapeMismatchError),
    /// The target type is not a struct
    NotAStruct {
        /// The shape we tried to deserialize into
        shape: &'static Shape,
    },
    /// Unsupported field type
    UnsupportedType {
        /// Name of the field
        field: String,
        /// The shape of the field
        shape: &'static Shape,
    },
    /// JSONB deserialization error
    Jsonb {
        /// Name of the column
        column: String,
        /// Error message
        message: String,
    },
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::MissingColumn { column } => write!(f, "missing column: {column}"),
            Error::TypeMismatch {
                column, expected, ..
            } => {
                write!(
                    f,
                    "type mismatch for column '{column}': expected {expected}"
                )
            }
            Error::Reflect(e) => write!(f, "reflection error: {e}"),
            Error::Alloc(e) => write!(f, "allocation error: {e}"),
            Error::ShapeMismatch(e) => write!(f, "shape mismatch: {e}"),
            Error::NotAStruct { shape } => {
                write!(f, "cannot deserialize row into non-struct type: {shape}")
            }
            Error::UnsupportedType { field, shape } => {
                write!(f, "unsupported type for field '{field}': {shape}")
            }
            Error::Jsonb { column, message } => {
                write!(f, "JSONB error for column '{column}': {message}")
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::TypeMismatch { source, .. } => Some(source),
            Error::Reflect(e) => Some(e),
            Error::Alloc(e) => Some(e),
            Error::ShapeMismatch(e) => Some(e),
            _ => None,
        }
    }
}

impl From<ReflectError> for Error {
    fn from(e: ReflectError) -> Self {
        Error::Reflect(e)
    }
}

impl From<AllocError> for Error {
    fn from(e: AllocError) -> Self {
        Error::Alloc(e)
    }
}

impl From<ShapeMismatchError> for Error {
    fn from(e: ShapeMismatchError) -> Self {
        Error::ShapeMismatch(e)
    }
}

/// Result type for Row deserialization.
pub type Result<T> = core::result::Result<T, Error>;

/// Deserialize a tokio-postgres Row into any type implementing Facet.
///
/// The type must be a struct with named fields. Each field name is used to look up
/// the corresponding column in the row.
///
/// # Example
///
/// ```ignore
/// use facet::Facet;
/// use facet_tokio_postgres::from_row;
///
/// #[derive(Debug, Facet)]
/// struct User {
///     id: i32,
///     name: String,
///     active: bool,
/// }
///
/// let row = client.query_one("SELECT id, name, active FROM users LIMIT 1", &[]).await?;
/// let user: User = from_row(&row)?;
/// ```
pub fn from_row<T: Facet<'static>>(row: &Row) -> Result<T> {
    let partial = Partial::alloc_owned::<T>()?;
    let partial = deserialize_row_into(row, partial, T::SHAPE)?;
    let heap_value = partial.build()?;
    Ok(heap_value.materialize()?)
}

/// Internal function to deserialize a row into a Partial.
fn deserialize_row_into(
    row: &Row,
    partial: Partial<'static, false>,
    shape: &'static Shape,
) -> Result<Partial<'static, false>> {
    let struct_def = match &shape.ty {
        Type::User(UserType::Struct(s)) if s.kind == StructKind::Struct => s,
        _ => {
            return Err(Error::NotAStruct { shape });
        }
    };

    let mut partial = partial;
    let num_fields = struct_def.fields.len();
    let mut fields_set = alloc::vec![false; num_fields];

    for (idx, field) in struct_def.fields.iter().enumerate() {
        let column_name = field.rename.unwrap_or(field.name);

        // Check if column exists
        let column_idx = match row.columns().iter().position(|c| c.name() == column_name) {
            Some(idx) => idx,
            None => {
                // Try to set default for missing column
                partial =
                    partial
                        .set_nth_field_to_default(idx)
                        .map_err(|_| Error::MissingColumn {
                            column: column_name.to_string(),
                        })?;
                fields_set[idx] = true;
                continue;
            }
        };

        partial = partial.begin_field(field.name)?;
        partial = deserialize_column(row, column_idx, column_name, partial, field.shape())?;
        partial = partial.end()?;
        fields_set[idx] = true;
    }

    Ok(partial)
}

/// Deserialize a single column value into a Partial.
fn deserialize_column(
    row: &Row,
    column_idx: usize,
    column_name: &str,
    partial: Partial<'static, false>,
    shape: &'static Shape,
) -> Result<Partial<'static, false>> {
    let mut partial = partial;

    // Handle Option types first - check via decl_id
    if shape.decl_id == Option::<()>::SHAPE.decl_id {
        return deserialize_option_column(row, column_idx, column_name, partial, shape);
    }

    // Handle based on type
    match &shape.ty {
        // Signed integers - compare shapes directly
        _ if shape == i8::SHAPE => {
            let val: i8 = get_column(row, column_idx, column_name, shape)?;
            partial = partial.set(val)?;
        }
        _ if shape == i16::SHAPE => {
            let val: i16 = get_column(row, column_idx, column_name, shape)?;
            partial = partial.set(val)?;
        }
        _ if shape == i32::SHAPE => {
            let val: i32 = get_column(row, column_idx, column_name, shape)?;
            partial = partial.set(val)?;
        }
        _ if shape == i64::SHAPE => {
            let val: i64 = get_column(row, column_idx, column_name, shape)?;
            partial = partial.set(val)?;
        }

        // Unsigned integers (postgres doesn't have native unsigned, but we can try)
        // We read as the next larger signed type and convert
        _ if shape == u8::SHAPE => {
            let val: i16 = get_column(row, column_idx, column_name, shape)?;
            partial = partial.set(val as u8)?;
        }
        _ if shape == u16::SHAPE => {
            let val: i32 = get_column(row, column_idx, column_name, shape)?;
            partial = partial.set(val as u16)?;
        }
        _ if shape == u32::SHAPE => {
            let val: i64 = get_column(row, column_idx, column_name, shape)?;
            partial = partial.set(val as u32)?;
        }
        _ if shape == u64::SHAPE => {
            // For u64, we use BIGINT and hope it fits
            let val: i64 = get_column(row, column_idx, column_name, shape)?;
            partial = partial.set(val as u64)?;
        }

        // Floats
        _ if shape == f32::SHAPE => {
            let val: f32 = get_column(row, column_idx, column_name, shape)?;
            partial = partial.set(val)?;
        }
        _ if shape == f64::SHAPE => {
            let val: f64 = get_column(row, column_idx, column_name, shape)?;
            partial = partial.set(val)?;
        }

        // Booleans
        _ if shape == bool::SHAPE => {
            let val: bool = get_column(row, column_idx, column_name, shape)?;
            partial = partial.set(val)?;
        }

        // Strings
        _ if shape == String::SHAPE => {
            let val: String = get_column(row, column_idx, column_name, shape)?;
            partial = partial.set(val)?;
        }

        // Vec<u8> for bytea
        _ if shape == <Vec<u8>>::SHAPE => {
            let val: Vec<u8> = get_column(row, column_idx, column_name, shape)?;
            partial = partial.set(val)?;
        }

        // Vec<String> for TEXT[]
        _ if shape == <Vec<String>>::SHAPE => {
            let val: Vec<String> = get_column(row, column_idx, column_name, shape)?;
            partial = partial.set(val)?;
        }

        // Vec<i64> for BIGINT[]
        _ if shape == <Vec<i64>>::SHAPE => {
            let val: Vec<i64> = get_column(row, column_idx, column_name, shape)?;
            partial = partial.set(val)?;
        }

        // Vec<i32> for INTEGER[]
        _ if shape == <Vec<i32>>::SHAPE => {
            let val: Vec<i32> = get_column(row, column_idx, column_name, shape)?;
            partial = partial.set(val)?;
        }

        // rust_decimal::Decimal for NUMERIC columns
        #[cfg(feature = "rust_decimal")]
        _ if shape == rust_decimal::Decimal::SHAPE => {
            let val: rust_decimal::Decimal = get_column(row, column_idx, column_name, shape)?;
            partial = partial.set(val)?;
        }

        // jiff::Timestamp for TIMESTAMPTZ columns
        #[cfg(feature = "jiff02")]
        _ if shape == jiff::Timestamp::SHAPE => {
            let val: jiff::Timestamp = get_column(row, column_idx, column_name, shape)?;
            partial = partial.set(val)?;
        }

        // jiff::civil::DateTime for TIMESTAMP (without timezone) columns
        #[cfg(feature = "jiff02")]
        _ if shape == jiff::civil::DateTime::SHAPE => {
            let val: jiff::civil::DateTime = get_column(row, column_idx, column_name, shape)?;
            partial = partial.set(val)?;
        }

        // JSONB columns via Jsonb<T> wrapper
        _ if shape.decl_id == Jsonb::<()>::SHAPE.decl_id => {
            partial = deserialize_jsonb_column(row, column_idx, column_name, partial, shape)?;
        }

        // Fallback: try to use parse if the type supports it
        _ => {
            if shape.vtable.has_parse() {
                // Try getting as string and parsing
                let val: String = get_column(row, column_idx, column_name, shape)?;
                partial = partial.parse_from_str(&val)?;
            } else {
                return Err(Error::UnsupportedType {
                    field: column_name.to_string(),
                    shape,
                });
            }
        }
    }

    Ok(partial)
}

/// Deserialize an Option column.
fn deserialize_option_column(
    row: &Row,
    column_idx: usize,
    column_name: &str,
    partial: Partial<'static, false>,
    shape: &'static Shape,
) -> Result<Partial<'static, false>> {
    let inner_shape = shape.inner.expect("Option must have inner shape");
    let mut partial = partial;

    // Try to get the value directly as Option<T> for the appropriate type
    // This handles NULL detection properly for each type
    macro_rules! try_option {
        ($t:ty) => {{
            let val: Option<$t> = get_column(row, column_idx, column_name, shape)?;
            match val {
                Some(v) => {
                    partial = partial.begin_some()?;
                    partial = partial.set(v)?;
                    partial = partial.end()?;
                }
                None => {
                    partial = partial.set_default()?;
                }
            }
            return Ok(partial);
        }};
    }

    // Macro for unsigned types that need conversion from larger signed types
    macro_rules! try_option_unsigned {
        ($signed:ty, $unsigned:ty) => {{
            let val: Option<$signed> = get_column(row, column_idx, column_name, shape)?;
            match val {
                Some(v) => {
                    partial = partial.begin_some()?;
                    partial = partial.set(v as $unsigned)?;
                    partial = partial.end()?;
                }
                None => {
                    partial = partial.set_default()?;
                }
            }
            return Ok(partial);
        }};
    }

    // Match on inner shape directly
    if inner_shape == i8::SHAPE {
        try_option!(i8);
    } else if inner_shape == i16::SHAPE {
        try_option!(i16);
    } else if inner_shape == i32::SHAPE {
        try_option!(i32);
    } else if inner_shape == i64::SHAPE {
        try_option!(i64);
    } else if inner_shape == u8::SHAPE {
        try_option_unsigned!(i16, u8);
    } else if inner_shape == u16::SHAPE {
        try_option_unsigned!(i32, u16);
    } else if inner_shape == u32::SHAPE {
        try_option_unsigned!(i64, u32);
    } else if inner_shape == u64::SHAPE {
        try_option_unsigned!(i64, u64);
    } else if inner_shape == f32::SHAPE {
        try_option!(f32);
    } else if inner_shape == f64::SHAPE {
        try_option!(f64);
    } else if inner_shape == bool::SHAPE {
        try_option!(bool);
    } else if inner_shape == String::SHAPE {
        try_option!(String);
    }

    #[cfg(feature = "rust_decimal")]
    if inner_shape == rust_decimal::Decimal::SHAPE {
        try_option!(rust_decimal::Decimal);
    }

    #[cfg(feature = "jiff02")]
    if inner_shape == jiff::Timestamp::SHAPE {
        try_option!(jiff::Timestamp);
    }

    #[cfg(feature = "jiff02")]
    if inner_shape == jiff::civil::DateTime::SHAPE {
        try_option!(jiff::civil::DateTime);
    }

    // Option<Jsonb<T>> - use decl_id comparison for generic types
    if inner_shape.decl_id == Jsonb::<()>::SHAPE.decl_id {
        // Read JSONB as optional raw bytes using our custom OptionalRawJsonb type
        let val: OptionalRawJsonb = get_column(row, column_idx, column_name, shape)?;
        match val.0 {
            Some(raw_bytes) => {
                partial = partial.begin_some()?;
                partial = deserialize_jsonb_bytes(&raw_bytes, partial, inner_shape, column_name)?;
                partial = partial.end()?;
            }
            None => {
                partial = partial.set_default()?;
            }
        }
        return Ok(partial);
    }

    // Fallback: try String and parse
    if inner_shape.vtable.has_parse() {
        let val: Option<String> = get_column(row, column_idx, column_name, shape)?;
        match val {
            Some(s) => {
                partial = partial.begin_some()?;
                partial = partial.parse_from_str(&s)?;
                partial = partial.end()?;
            }
            None => {
                partial = partial.set_default()?;
            }
        }
        return Ok(partial);
    }

    Err(Error::UnsupportedType {
        field: column_name.to_string(),
        shape: inner_shape,
    })
}

/// Get a column value with proper error handling.
fn get_column<'a, T>(row: &'a Row, idx: usize, name: &str, shape: &'static Shape) -> Result<T>
where
    T: postgres_types::FromSql<'a>,
{
    row.try_get::<_, T>(idx).map_err(|e| Error::TypeMismatch {
        column: name.to_string(),
        expected: shape,
        source: e,
    })
}

/// Deserialize a JSONB column into a Jsonb<T> wrapper.
fn deserialize_jsonb_column(
    row: &Row,
    column_idx: usize,
    column_name: &str,
    partial: Partial<'static, false>,
    shape: &'static Shape,
) -> Result<Partial<'static, false>> {
    // Read JSONB as raw bytes from PostgreSQL using our custom RawJsonb type
    let raw_jsonb: RawJsonb = get_column(row, column_idx, column_name, shape)?;
    deserialize_jsonb_bytes(&raw_jsonb.0, partial, shape, column_name)
}

/// Deserialize JSONB bytes into a Jsonb<T> wrapper.
fn deserialize_jsonb_bytes(
    raw_bytes: &[u8],
    mut partial: Partial<'static, false>,
    _shape: &'static Shape,
    column_name: &str,
) -> Result<Partial<'static, false>> {
    if raw_bytes.is_empty() {
        return Err(Error::Jsonb {
            column: column_name.to_string(),
            message: "empty JSONB data".to_string(),
        });
    }

    // JSONB wire format: 1 byte version (0x01) + JSON text
    if raw_bytes[0] != 1 {
        return Err(Error::Jsonb {
            column: column_name.to_string(),
            message: format!("unsupported JSONB version: {}", raw_bytes[0]),
        });
    }

    // Skip version byte
    let json_bytes = &raw_bytes[1..];

    // Begin the Jsonb wrapper's inner field (field 0)
    partial = partial.begin_nth_field(0)?;

    // Use facet-json to deserialize directly into the inner type
    partial = facet_json::from_slice_into(json_bytes, partial).map_err(|e| Error::Jsonb {
        column: column_name.to_string(),
        message: format!("{e}"),
    })?;

    partial = partial.end()?;

    Ok(partial)
}
