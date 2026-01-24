//! JSONB support for PostgreSQL columns.
//!
//! This module provides the [`Jsonb<T>`] wrapper type for deserializing PostgreSQL
//! JSONB columns into Rust types that implement `Facet`.

use facet::Facet;
use postgres_types::{FromSql, Type};
use std::error::Error;
use std::fmt;
use std::ops::{Deref, DerefMut};

/// A wrapper type for PostgreSQL JSONB columns.
///
/// Use `Jsonb<T>` where `T` implements `Facet` to deserialize JSONB data.
/// For schemaless JSON, use `Jsonb<facet_value::Value>`.
#[derive(Clone, PartialEq, Eq, Facet)]
#[repr(transparent)]
pub struct Jsonb<T>(pub T);

impl<T> Jsonb<T> {
    /// Create a new `Jsonb` wrapper around the given value.
    #[inline]
    pub fn new(value: T) -> Self {
        Jsonb(value)
    }

    /// Unwrap the inner value.
    #[inline]
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> Deref for Jsonb<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for Jsonb<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> From<T> for Jsonb<T> {
    #[inline]
    fn from(value: T) -> Self {
        Jsonb(value)
    }
}

impl<T: fmt::Debug> fmt::Debug for Jsonb<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<T: Default> Default for Jsonb<T> {
    fn default() -> Self {
        Jsonb(T::default())
    }
}

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
