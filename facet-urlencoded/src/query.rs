//! The `Query<T>` wrapper type for URL query parameters.

use core::fmt;
use core::ops::{Deref, DerefMut};

/// A wrapper type for URL query parameter deserialization.
///
/// This type can be used standalone for convenient query string parsing,
/// and when the `axum` feature is enabled, it implements Axum's
/// `FromRequestParts` trait for extracting query parameters from the URL.
///
/// Unlike `Form<T>`, `Query<T>` does not consume the request body and can
/// be used alongside other extractors.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_urlencoded::Query;
///
/// #[derive(Debug, Facet)]
/// struct SearchParams {
///     q: String,
///     page: u64,
/// }
///
/// // Wrap a value
/// let query = Query(SearchParams {
///     q: "rust".to_string(),
///     page: 1,
/// });
///
/// // Access the inner value
/// println!("Search query: {}", query.q);
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Query<T>(pub T);

impl<T> Query<T> {
    /// Consume the wrapper and return the inner value.
    #[inline]
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> From<T> for Query<T> {
    #[inline]
    fn from(inner: T) -> Self {
        Query(inner)
    }
}

impl<T> Deref for Query<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for Query<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> AsRef<T> for Query<T> {
    #[inline]
    fn as_ref(&self) -> &T {
        &self.0
    }
}

impl<T> AsMut<T> for Query<T> {
    #[inline]
    fn as_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T: fmt::Display> fmt::Display for Query<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
