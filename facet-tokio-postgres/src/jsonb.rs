use postgres_types::{FromSql, Type};
use std::error::Error;

/// Internal type for reading raw JSONB bytes from PostgreSQL.
///
/// PostgreSQL JSONB columns can't be read as `Vec<u8>` directly because
/// they have different type OIDs. This wrapper implements `FromSql` to
/// accept both JSON and JSONB column types.
pub(crate) struct RawJsonb(pub Vec<u8>);

impl<'a> FromSql<'a> for RawJsonb {
    fn from_sql(ty: &Type, raw: &'a [u8]) -> Result<Self, Box<dyn Error + Sync + Send>> {
        // Accept both JSON and JSONB types
        if *ty == Type::JSON || *ty == Type::JSONB {
            Ok(RawJsonb(raw.to_vec()))
        } else {
            Err(format!("expected JSON or JSONB, got {:?}", ty).into())
        }
    }

    fn accepts(ty: &Type) -> bool {
        *ty == Type::JSON || *ty == Type::JSONB
    }
}

/// Internal type for reading optional raw JSONB bytes from PostgreSQL.
pub(crate) struct OptionalRawJsonb(pub Option<Vec<u8>>);

impl<'a> FromSql<'a> for OptionalRawJsonb {
    fn from_sql(ty: &Type, raw: &'a [u8]) -> Result<Self, Box<dyn Error + Sync + Send>> {
        RawJsonb::from_sql(ty, raw).map(|r| OptionalRawJsonb(Some(r.0)))
    }

    fn from_sql_null(_ty: &Type) -> Result<Self, Box<dyn Error + Sync + Send>> {
        Ok(OptionalRawJsonb(None))
    }

    fn accepts(ty: &Type) -> bool {
        RawJsonb::accepts(ty)
    }
}
