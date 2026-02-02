//! JSONB support for PostgreSQL columns.
//!
//! This module provides the [`Jsonb<T>`] wrapper type for deserializing PostgreSQL
//! JSONB columns into Rust types that implement `Facet`.

use facet::Facet;
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
