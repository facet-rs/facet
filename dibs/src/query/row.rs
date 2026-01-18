//! Row mapping between Postgres and Rust types.

use super::Value;
use crate::schema::PgType;
use tokio_postgres::types::{ToSql, Type as PgTypeInfo};

/// A row of data as field name → value pairs.
pub type Row = Vec<(String, Value)>;

/// Convert a tokio_postgres Row to our Row type.
pub fn pg_row_to_row(
    pg_row: &tokio_postgres::Row,
    columns: &[(String, PgType)],
) -> Result<Row, crate::Error> {
    let mut row = Vec::with_capacity(columns.len());

    for (i, (name, pg_type)) in columns.iter().enumerate() {
        let value = pg_value_to_value(pg_row, i, *pg_type)?;
        row.push((name.clone(), value));
    }

    Ok(row)
}

/// Extract a value from a Postgres row at a given index.
fn pg_value_to_value(
    row: &tokio_postgres::Row,
    idx: usize,
    pg_type: PgType,
) -> Result<Value, crate::Error> {
    // Check for NULL first
    // tokio_postgres returns None for NULL values when using get_opt
    match pg_type {
        PgType::Boolean => {
            let v: Option<bool> = row.get(idx);
            Ok(v.map(Value::Bool).unwrap_or(Value::Null))
        }
        PgType::SmallInt => {
            let v: Option<i16> = row.get(idx);
            Ok(v.map(Value::I16).unwrap_or(Value::Null))
        }
        PgType::Integer => {
            let v: Option<i32> = row.get(idx);
            Ok(v.map(Value::I32).unwrap_or(Value::Null))
        }
        PgType::BigInt => {
            let v: Option<i64> = row.get(idx);
            Ok(v.map(Value::I64).unwrap_or(Value::Null))
        }
        PgType::Real => {
            let v: Option<f32> = row.get(idx);
            Ok(v.map(Value::F32).unwrap_or(Value::Null))
        }
        PgType::DoublePrecision => {
            let v: Option<f64> = row.get(idx);
            Ok(v.map(Value::F64).unwrap_or(Value::Null))
        }
        PgType::Text => {
            let v: Option<String> = row.get(idx);
            Ok(v.map(Value::String).unwrap_or(Value::Null))
        }
        PgType::Bytea => {
            let v: Option<Vec<u8>> = row.get(idx);
            Ok(v.map(Value::Bytes).unwrap_or(Value::Null))
        }
        // TODO: Handle Timestamptz, Date, Time, Uuid, Jsonb
        _ => Err(crate::Error::UnsupportedType(format!("{:?}", pg_type))),
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
            Value::String(v) => v.to_sql(ty, out),
            Value::Bytes(v) => v.to_sql(ty, out),
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
                | PgTypeInfo::TEXT
                | PgTypeInfo::VARCHAR
                | PgTypeInfo::BYTEA
        )
    }

    tokio_postgres::types::to_sql_checked!();
}
