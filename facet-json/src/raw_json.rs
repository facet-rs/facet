//! Raw JSON value that defers parsing.
//!
//! [`RawJson`] captures unparsed JSON text, allowing you to delay or skip
//! deserialization of parts of a JSON document.

use alloc::borrow::Cow;
use alloc::string::String;
use core::fmt;
use facet::Facet;

/// A raw JSON value that has not been parsed.
///
/// This type captures the raw JSON text for a value, deferring parsing until
/// you're ready (or skipping it entirely if you just need to pass it through).
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_json::RawJson;
///
/// #[derive(Facet, Debug)]
/// struct Response<'a> {
///     status: u32,
///     // We don't know what shape `data` has, so defer parsing
///     data: RawJson<'a>,
/// }
///
/// let json = r#"{"status": 200, "data": {"nested": [1, 2, 3], "complex": true}}"#;
/// let response: Response = facet_json::from_str_borrowed(json).unwrap();
///
/// assert_eq!(response.status, 200);
/// assert_eq!(response.data.as_str(), r#"{"nested": [1, 2, 3], "complex": true}"#);
/// ```
#[derive(Clone, PartialEq, Eq, Hash, Facet)]
pub struct RawJson<'a>(pub Cow<'a, str>);

impl<'a> RawJson<'a> {
    /// Create a new `RawJson` from a string slice.
    #[inline]
    pub const fn new(s: &'a str) -> Self {
        RawJson(Cow::Borrowed(s))
    }

    /// Create a new `RawJson` from an owned string.
    #[inline]
    pub const fn from_owned(s: String) -> Self {
        RawJson(Cow::Owned(s))
    }

    /// Get the raw JSON as a string slice.
    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Convert into an owned `RawJson<'static>`.
    #[inline]
    pub fn into_owned(self) -> RawJson<'static> {
        RawJson(Cow::Owned(self.0.into_owned()))
    }
}

impl fmt::Debug for RawJson<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("RawJson").field(&self.0).finish()
    }
}

impl fmt::Display for RawJson<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'a> From<&'a str> for RawJson<'a> {
    fn from(s: &'a str) -> Self {
        RawJson::new(s)
    }
}

impl<'a> From<Cow<'a, str>> for RawJson<'a> {
    fn from(s: Cow<'a, str>) -> Self {
        RawJson(s)
    }
}

impl From<String> for RawJson<'static> {
    fn from(s: String) -> Self {
        RawJson::from_owned(s)
    }
}

impl<'a> AsRef<str> for RawJson<'a> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}
