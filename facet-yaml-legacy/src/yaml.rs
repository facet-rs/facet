//! The `Yaml<T>` wrapper type for YAML serialization/deserialization.

use core::fmt;
use core::ops::{Deref, DerefMut};

/// A wrapper type for YAML serialization and deserialization.
///
/// When the `axum` feature is enabled, this type implements Axum's
/// `FromRequest` and `IntoResponse` traits.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Yaml<T>(pub T);

impl<T> Yaml<T> {
    /// Consume the wrapper and return the inner value.
    #[inline]
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> From<T> for Yaml<T> {
    #[inline]
    fn from(inner: T) -> Self {
        Yaml(inner)
    }
}

impl<T> Deref for Yaml<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for Yaml<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T: fmt::Display> fmt::Display for Yaml<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
