//! The `Form<T>` wrapper type for URL-encoded form data.

use core::fmt;
use core::ops::{Deref, DerefMut};

/// A wrapper type for URL-encoded form data deserialization.
///
/// This type can be used standalone for convenient form parsing,
/// and when the `axum` feature is enabled, it implements Axum's
/// `FromRequest` trait for extracting form data from request bodies.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_urlencoded::Form;
///
/// #[derive(Debug, Facet)]
/// struct LoginForm {
///     username: String,
///     password: String,
/// }
///
/// // Wrap a value
/// let form = Form(LoginForm {
///     username: "alice".to_string(),
///     password: "secret".to_string(),
/// });
///
/// // Access the inner value
/// println!("Username: {}", form.username);
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Form<T>(pub T);

impl<T> Form<T> {
    /// Consume the wrapper and return the inner value.
    #[inline]
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> From<T> for Form<T> {
    #[inline]
    fn from(inner: T) -> Self {
        Form(inner)
    }
}

impl<T> Deref for Form<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for Form<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> AsRef<T> for Form<T> {
    #[inline]
    fn as_ref(&self) -> &T {
        &self.0
    }
}

impl<T> AsMut<T> for Form<T> {
    #[inline]
    fn as_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T: fmt::Display> fmt::Display for Form<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
