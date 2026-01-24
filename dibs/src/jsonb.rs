//! JSONB column type for PostgreSQL.
//!
//! Use `Jsonb<T>` to store data as JSONB in PostgreSQL. The type parameter `T`
//! must implement `Facet` for serialization/deserialization.
//!
//! # Examples
//!
//! ```ignore
//! use dibs::Jsonb;
//! use facet::Facet;
//!
//! // Typed JSONB - schema is known
//! #[derive(Debug, Clone, Facet)]
//! struct PrintifyData {
//!     id: String,
//!     title: String,
//!     variants: Vec<Variant>,
//! }
//!
//! #[derive(Debug, Clone, Facet)]
//! #[facet(derive(dibs::Table))]
//! struct Product {
//!     #[facet(dibs::pk)]
//!     id: i64,
//!     // Stored as JSONB, typed as PrintifyData
//!     printify_data: Jsonb<PrintifyData>,
//! }
//!
//! // Schemaless JSONB - arbitrary JSON
//! use facet_value::Value;
//!
//! #[derive(Debug, Clone, Facet)]
//! #[facet(derive(dibs::Table))]
//! struct Event {
//!     #[facet(dibs::pk)]
//!     id: i64,
//!     // Stored as JSONB, can hold any JSON value
//!     metadata: Option<Jsonb<Value>>,
//! }
//! ```

use facet::Facet;
use std::fmt;
use std::ops::{Deref, DerefMut};

/// A wrapper type that maps to PostgreSQL's JSONB column type.
///
/// The inner type `T` must implement `Facet` for serialization and deserialization.
/// Use `Jsonb<facet_value::Value>` for schemaless/arbitrary JSON.
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
