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

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use facet_core::{Facet, Shape, StructKind, Type, UserType};
use facet_reflect::{Partial, ReflectError};
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
            Error::NotAStruct { shape } => {
                write!(f, "cannot deserialize row into non-struct type: {shape}")
            }
            Error::UnsupportedType { field, shape } => {
                write!(f, "unsupported type for field '{field}': {shape}")
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::TypeMismatch { source, .. } => Some(source),
            Error::Reflect(e) => Some(e),
            _ => None,
        }
    }
}

impl From<ReflectError> for Error {
    fn from(e: ReflectError) -> Self {
        Error::Reflect(e)
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
pub fn from_row<'facet, T: Facet<'facet>>(row: &Row) -> Result<T> {
    let partial = Partial::alloc::<T>()?;
    let partial = deserialize_row_into(row, partial, T::SHAPE)?;
    let heap_value = partial.build()?;
    Ok(heap_value.materialize()?)
}

/// Internal function to deserialize a row into a Partial.
fn deserialize_row_into<'p>(
    row: &Row,
    partial: Partial<'p>,
    shape: &'static Shape,
) -> Result<Partial<'p>> {
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
fn deserialize_column<'p>(
    row: &Row,
    column_idx: usize,
    column_name: &str,
    partial: Partial<'p>,
    shape: &'static Shape,
) -> Result<Partial<'p>> {
    use facet_core::{Def, NumericType, PrimitiveType};

    let mut partial = partial;

    // Handle Option types first
    if let Def::Option(_) = &shape.def {
        return deserialize_option_column(row, column_idx, column_name, partial, shape);
    }

    // Handle based on type
    match &shape.ty {
        // Signed integers
        Type::Primitive(PrimitiveType::Numeric(NumericType::Integer { signed: true })) => {
            match shape.type_identifier {
                "i8" => {
                    let val: i8 = get_column(row, column_idx, column_name, shape)?;
                    partial = partial.set(val)?;
                }
                "i16" => {
                    let val: i16 = get_column(row, column_idx, column_name, shape)?;
                    partial = partial.set(val)?;
                }
                "i32" => {
                    let val: i32 = get_column(row, column_idx, column_name, shape)?;
                    partial = partial.set(val)?;
                }
                "i64" => {
                    let val: i64 = get_column(row, column_idx, column_name, shape)?;
                    partial = partial.set(val)?;
                }
                _ => {
                    return Err(Error::UnsupportedType {
                        field: column_name.to_string(),
                        shape,
                    });
                }
            }
        }

        // Unsigned integers (postgres doesn't have native unsigned, but we can try)
        Type::Primitive(PrimitiveType::Numeric(NumericType::Integer { signed: false })) => {
            // Postgres doesn't have unsigned types natively, so we read as signed and convert
            match shape.type_identifier {
                "u8" => {
                    let val: i16 = get_column(row, column_idx, column_name, shape)?;
                    partial = partial.set(val as u8)?;
                }
                "u16" => {
                    let val: i32 = get_column(row, column_idx, column_name, shape)?;
                    partial = partial.set(val as u16)?;
                }
                "u32" => {
                    let val: i64 = get_column(row, column_idx, column_name, shape)?;
                    partial = partial.set(val as u32)?;
                }
                "u64" => {
                    // For u64, we might need to use BIGINT and hope it fits
                    let val: i64 = get_column(row, column_idx, column_name, shape)?;
                    partial = partial.set(val as u64)?;
                }
                _ => {
                    return Err(Error::UnsupportedType {
                        field: column_name.to_string(),
                        shape,
                    });
                }
            }
        }

        // Floats
        Type::Primitive(PrimitiveType::Numeric(NumericType::Float)) => {
            match shape.type_identifier {
                "f32" => {
                    let val: f32 = get_column(row, column_idx, column_name, shape)?;
                    partial = partial.set(val)?;
                }
                "f64" => {
                    let val: f64 = get_column(row, column_idx, column_name, shape)?;
                    partial = partial.set(val)?;
                }
                _ => {
                    return Err(Error::UnsupportedType {
                        field: column_name.to_string(),
                        shape,
                    });
                }
            }
        }

        // Booleans
        Type::Primitive(PrimitiveType::Boolean) => {
            let val: bool = get_column(row, column_idx, column_name, shape)?;
            partial = partial.set(val)?;
        }

        // Strings
        Type::Primitive(PrimitiveType::Textual(_)) | Type::User(_)
            if shape.type_identifier == "String" =>
        {
            let val: String = get_column(row, column_idx, column_name, shape)?;
            partial = partial.set(val)?;
        }

        // Vec<u8> for bytea - check if it's a List of u8
        _ if matches!(&shape.def, Def::List(_))
            && shape
                .inner
                .is_some_and(|inner| inner.type_identifier == "u8") =>
        {
            let val: Vec<u8> = get_column(row, column_idx, column_name, shape)?;
            partial = partial.set(val)?;
        }

        // rust_decimal::Decimal for NUMERIC columns
        #[cfg(feature = "rust_decimal")]
        _ if shape.type_identifier == "Decimal" => {
            let val: rust_decimal::Decimal = get_column(row, column_idx, column_name, shape)?;
            partial = partial.set(val)?;
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
fn deserialize_option_column<'p>(
    row: &Row,
    column_idx: usize,
    column_name: &str,
    partial: Partial<'p>,
    shape: &'static Shape,
) -> Result<Partial<'p>> {
    use facet_core::{NumericType, PrimitiveType};

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

    // Match on inner type to get the right Option<T>
    match &inner_shape.ty {
        Type::Primitive(PrimitiveType::Numeric(NumericType::Integer { signed: true })) => {
            match inner_shape.type_identifier {
                "i8" => try_option!(i8),
                "i16" => try_option!(i16),
                "i32" => try_option!(i32),
                "i64" => try_option!(i64),
                _ => {}
            }
        }
        Type::Primitive(PrimitiveType::Numeric(NumericType::Integer { signed: false })) => {
            // Postgres doesn't have unsigned, read as next larger signed type
            match inner_shape.type_identifier {
                "u8" => {
                    let val: Option<i16> = get_column(row, column_idx, column_name, shape)?;
                    match val {
                        Some(v) => {
                            partial = partial.begin_some()?;
                            partial = partial.set(v as u8)?;
                            partial = partial.end()?;
                        }
                        None => {
                            partial = partial.set_default()?;
                        }
                    }
                    return Ok(partial);
                }
                "u16" => {
                    let val: Option<i32> = get_column(row, column_idx, column_name, shape)?;
                    match val {
                        Some(v) => {
                            partial = partial.begin_some()?;
                            partial = partial.set(v as u16)?;
                            partial = partial.end()?;
                        }
                        None => {
                            partial = partial.set_default()?;
                        }
                    }
                    return Ok(partial);
                }
                "u32" => {
                    let val: Option<i64> = get_column(row, column_idx, column_name, shape)?;
                    match val {
                        Some(v) => {
                            partial = partial.begin_some()?;
                            partial = partial.set(v as u32)?;
                            partial = partial.end()?;
                        }
                        None => {
                            partial = partial.set_default()?;
                        }
                    }
                    return Ok(partial);
                }
                "u64" => {
                    let val: Option<i64> = get_column(row, column_idx, column_name, shape)?;
                    match val {
                        Some(v) => {
                            partial = partial.begin_some()?;
                            partial = partial.set(v as u64)?;
                            partial = partial.end()?;
                        }
                        None => {
                            partial = partial.set_default()?;
                        }
                    }
                    return Ok(partial);
                }
                _ => {}
            }
        }
        Type::Primitive(PrimitiveType::Numeric(NumericType::Float)) => {
            match inner_shape.type_identifier {
                "f32" => try_option!(f32),
                "f64" => try_option!(f64),
                _ => {}
            }
        }
        Type::Primitive(PrimitiveType::Boolean) => try_option!(bool),
        _ if inner_shape.type_identifier == "String" => try_option!(String),
        #[cfg(feature = "rust_decimal")]
        _ if inner_shape.type_identifier == "Decimal" => try_option!(rust_decimal::Decimal),
        _ => {}
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
