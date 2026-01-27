//! Row mapping between Postgres and Rust types.

use super::Value;
use crate::schema::PgType;
use rust_decimal::Decimal;
use std::error::Error as StdError;
use tokio_postgres::types::{FromSql, ToSql, Type as PgTypeInfo, WrongType};

/// Internal type for reading raw JSONB bytes from PostgreSQL.
struct JsonbRaw(Vec<u8>);

impl<'a> FromSql<'a> for JsonbRaw {
    fn from_sql(
        ty: &PgTypeInfo,
        raw: &'a [u8],
    ) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        if *ty == PgTypeInfo::JSON || *ty == PgTypeInfo::JSONB {
            Ok(JsonbRaw(raw.to_vec()))
        } else {
            Err(format!("expected JSON or JSONB, got {:?}", ty).into())
        }
    }

    fn accepts(ty: &PgTypeInfo) -> bool {
        *ty == PgTypeInfo::JSON || *ty == PgTypeInfo::JSONB
    }
}

/// Optional version for handling NULL JSONB values.
struct OptionalJsonbRaw(Option<Vec<u8>>);

impl<'a> FromSql<'a> for OptionalJsonbRaw {
    fn from_sql(
        ty: &PgTypeInfo,
        raw: &'a [u8],
    ) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        JsonbRaw::from_sql(ty, raw).map(|r| OptionalJsonbRaw(Some(r.0)))
    }

    fn from_sql_null(_ty: &PgTypeInfo) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        Ok(OptionalJsonbRaw(None))
    }

    fn accepts(ty: &PgTypeInfo) -> bool {
        JsonbRaw::accepts(ty)
    }
}

/// A row of data as field name → value pairs.
pub type Row = Vec<(String, Value)>;

/// Context for error reporting when reading rows.
#[derive(Clone)]
pub struct RowContext<'a> {
    pub table_name: &'a str,
}

/// Convert a tokio_postgres Row to our Row type.
pub fn pg_row_to_row(
    pg_row: &tokio_postgres::Row,
    columns: &[(String, PgType)],
    ctx: &RowContext<'_>,
) -> Result<Row, crate::Error> {
    let mut row = Vec::with_capacity(columns.len());

    for (i, (name, pg_type)) in columns.iter().enumerate() {
        let value = pg_value_to_value(pg_row, i, name, *pg_type, ctx)?;
        row.push((name.clone(), value));
    }

    Ok(row)
}

/// Extract a value from a Postgres row at a given index.
fn pg_value_to_value(
    row: &tokio_postgres::Row,
    idx: usize,
    column_name: &str,
    pg_type: PgType,
    ctx: &RowContext<'_>,
) -> Result<Value, crate::Error> {
    // Helper to create a type mismatch error
    let type_mismatch = |expected: &str| {
        let actual = row
            .columns()
            .get(idx)
            .map(|c| c.type_().name().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        crate::Error::TypeMismatch {
            table: ctx.table_name.to_string(),
            column: column_name.to_string(),
            expected: expected.to_string(),
            actual,
        }
    };

    // Helper to create an error when reading a column fails.
    // We check if the underlying error is a WrongType error (type mismatch between
    // what Rust expects and what the database has).
    let read_error = |expected: &str, e: tokio_postgres::Error| {
        // Check if the source error is a WrongType - this means the database column
        // type doesn't match what the Rust code tried to deserialize as.
        if e.source()
            .and_then(|s| s.downcast_ref::<WrongType>())
            .is_some()
        {
            // WrongType's Display shows "cannot convert between the Rust type `X` and the Postgres type `Y`"
            // We can extract the postgres type from the column metadata
            let actual_type = row
                .columns()
                .get(idx)
                .map(|c| c.type_().name().to_string())
                .unwrap_or_else(|| "unknown".to_string());

            return crate::Error::TypeMismatch {
                table: ctx.table_name.to_string(),
                column: column_name.to_string(),
                expected: expected.to_string(),
                actual: actual_type,
            };
        }

        // Not a WrongType error - some other deserialization issue
        crate::Error::ColumnReadError {
            table: ctx.table_name.to_string(),
            column: column_name.to_string(),
            expected: expected.to_string(),
            message: e.to_string(),
        }
    };
    match pg_type {
        PgType::Boolean => {
            let v: Option<bool> = row.try_get(idx).map_err(|e| read_error("bool", e))?;
            Ok(v.map(Value::Bool).unwrap_or(Value::Null))
        }
        PgType::SmallInt => {
            let v: Option<i16> = row.try_get(idx).map_err(|e| read_error("smallint", e))?;
            Ok(v.map(Value::I16).unwrap_or(Value::Null))
        }
        PgType::Integer => {
            let v: Option<i32> = row.try_get(idx).map_err(|e| read_error("integer", e))?;
            Ok(v.map(Value::I32).unwrap_or(Value::Null))
        }
        PgType::BigInt => {
            let v: Option<i64> = row.try_get(idx).map_err(|e| read_error("bigint", e))?;
            Ok(v.map(Value::I64).unwrap_or(Value::Null))
        }
        PgType::Real => {
            let v: Option<f32> = row.try_get(idx).map_err(|e| read_error("real", e))?;
            Ok(v.map(Value::F32).unwrap_or(Value::Null))
        }
        PgType::DoublePrecision => {
            let v: Option<f64> = row
                .try_get(idx)
                .map_err(|e| read_error("double precision", e))?;
            Ok(v.map(Value::F64).unwrap_or(Value::Null))
        }
        PgType::Numeric => {
            let v: Option<Decimal> = row.try_get(idx).map_err(|e| read_error("numeric", e))?;
            Ok(v.map(Value::Decimal).unwrap_or(Value::Null))
        }
        PgType::Text => {
            let v: Option<String> = row.try_get(idx).map_err(|e| read_error("text", e))?;
            Ok(v.map(Value::String).unwrap_or(Value::Null))
        }
        PgType::Bytea => {
            let v: Option<Vec<u8>> = row.try_get(idx).map_err(|e| read_error("bytea", e))?;
            Ok(v.map(Value::Bytes).unwrap_or(Value::Null))
        }
        PgType::Timestamptz => {
            let v: Option<std::time::SystemTime> =
                row.try_get(idx).map_err(|e| read_error("timestamptz", e))?;
            match v {
                Some(st) => {
                    let datetime: chrono::DateTime<chrono::Utc> = st.into();
                    Ok(Value::String(datetime.to_rfc3339()))
                }
                None => Ok(Value::Null),
            }
        }
        PgType::Date => {
            let v: Option<chrono::NaiveDate> =
                row.try_get(idx).map_err(|e| read_error("date", e))?;
            match v {
                Some(d) => Ok(Value::String(d.to_string())),
                None => Ok(Value::Null),
            }
        }
        PgType::Time => {
            let v: Option<chrono::NaiveTime> =
                row.try_get(idx).map_err(|e| read_error("time", e))?;
            match v {
                Some(t) => Ok(Value::String(t.to_string())),
                None => Ok(Value::Null),
            }
        }
        PgType::Jsonb => {
            let v: OptionalJsonbRaw = row.try_get(idx).map_err(|e| read_error("jsonb", e))?;
            match v.0 {
                Some(raw) => {
                    // JSONB wire format has a 1-byte version prefix, skip it
                    let json_bytes = if raw.first() == Some(&1) {
                        &raw[1..]
                    } else {
                        &raw
                    };
                    let json_str = String::from_utf8_lossy(json_bytes).into_owned();
                    Ok(Value::Json(json_str))
                }
                None => Ok(Value::Null),
            }
        }
        _ => Err(type_mismatch(&format!("{:?}", pg_type))),
    }
}

/// Wrapper to make our Value usable as a ToSql parameter.
#[derive(Debug)]
pub struct SqlParam<'a>(pub &'a Value);

impl ToSql for SqlParam<'_> {
    fn to_sql(
        &self,
        ty: &PgTypeInfo,
        out: &mut bytes::BytesMut,
    ) -> Result<tokio_postgres::types::IsNull, Box<dyn std::error::Error + Sync + Send>> {
        match self.0 {
            Value::Null => Ok(tokio_postgres::types::IsNull::Yes),
            Value::Bool(v) => v.to_sql(ty, out),
            Value::I16(v) => v.to_sql(ty, out),
            Value::I32(v) => v.to_sql(ty, out),
            Value::I64(v) => v.to_sql(ty, out),
            Value::F32(v) => v.to_sql(ty, out),
            Value::F64(v) => v.to_sql(ty, out),
            Value::Decimal(v) => v.to_sql(ty, out),
            Value::String(v) => v.to_sql(ty, out),
            Value::Bytes(v) => v.to_sql(ty, out),
            Value::Json(v) => {
                // For JSONB, we need to prepend the version byte
                if *ty == PgTypeInfo::JSONB {
                    out.extend_from_slice(&[1]); // JSONB version 1
                }
                out.extend_from_slice(v.as_bytes());
                Ok(tokio_postgres::types::IsNull::No)
            }
        }
    }

    fn accepts(ty: &PgTypeInfo) -> bool {
        // Accept common types
        matches!(
            *ty,
            PgTypeInfo::BOOL
                | PgTypeInfo::INT2
                | PgTypeInfo::INT4
                | PgTypeInfo::INT8
                | PgTypeInfo::FLOAT4
                | PgTypeInfo::FLOAT8
                | PgTypeInfo::NUMERIC
                | PgTypeInfo::TEXT
                | PgTypeInfo::VARCHAR
                | PgTypeInfo::BYTEA
                | PgTypeInfo::JSON
                | PgTypeInfo::JSONB
        )
    }

    tokio_postgres::types::to_sql_checked!();
}
