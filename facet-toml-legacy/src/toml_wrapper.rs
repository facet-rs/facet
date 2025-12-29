//! The `Toml<T>` wrapper type for TOML serialization/deserialization.

use core::fmt;
use core::ops::{Deref, DerefMut};

/// A wrapper type for TOML serialization and deserialization.
///
/// When the `axum` feature is enabled, this type implements Axum's
/// `FromRequest` and `IntoResponse` traits.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Toml<T>(pub T);

impl<T> Toml<T> {
    /// Consume the wrapper and return the inner value.
    #[inline]
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> From<T> for Toml<T> {
    #[inline]
    fn from(inner: T) -> Self {
        Toml(inner)
    }
}

impl<T> Deref for Toml<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for Toml<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T: fmt::Display> fmt::Display for Toml<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
