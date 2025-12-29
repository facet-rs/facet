//! The `Json<T>` wrapper type for JSON serialization/deserialization.

use core::fmt;
use core::ops::{Deref, DerefMut};

/// A wrapper type for JSON serialization and deserialization.
///
/// This type can be used standalone for convenient JSON operations,
/// and when the `axum` feature is enabled, it implements Axum's
/// `FromRequest` and `IntoResponse` traits.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_json_legacy::Json;
///
/// #[derive(Debug, Facet)]
/// struct User {
///     name: String,
///     age: u32,
/// }
///
/// // Wrap a value
/// let user = Json(User { name: "Alice".to_string(), age: 30 });
///
/// // Access the inner value
/// println!("Name: {}", user.name);
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Json<T>(pub T);

impl<T> Json<T> {
    /// Consume the wrapper and return the inner value.
    #[inline]
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> From<T> for Json<T> {
    #[inline]
    fn from(inner: T) -> Self {
        Json(inner)
    }
}

impl<T> Deref for Json<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for Json<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> AsRef<T> for Json<T> {
    #[inline]
    fn as_ref(&self) -> &T {
        &self.0
    }
}

impl<T> AsMut<T> for Json<T> {
    #[inline]
    fn as_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T: fmt::Display> fmt::Display for Json<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
