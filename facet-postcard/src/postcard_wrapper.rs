//! The `Postcard<T>` wrapper type for Postcard serialization/deserialization.

use core::fmt;
use core::ops::{Deref, DerefMut};

/// A wrapper type for Postcard serialization and deserialization.
///
/// Postcard is a `no_std` and embedded-systems friendly compact binary format.
/// When the `axum` feature is enabled, this type implements Axum's
/// `FromRequest` and `IntoResponse` traits.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Postcard<T>(pub T);

impl<T> Postcard<T> {
    /// Consume the wrapper and return the inner value.
    #[inline]
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> From<T> for Postcard<T> {
    #[inline]
    fn from(inner: T) -> Self {
        Postcard(inner)
    }
}

impl<T> Deref for Postcard<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for Postcard<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T: fmt::Display> fmt::Display for Postcard<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
