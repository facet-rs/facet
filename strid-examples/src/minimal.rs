//! An example of constructing a basic strongly-typed wrapper around
//! a custom string-like type.

use strid::braid;

/// A wrapper around a custom string-like type that implements the
/// minimal set of required traits for a braid type
#[braid(serde)]
pub struct MinimalUsernameBuf(MinimalString);

/// An example of a minimal string implementaiton that can be wrapped inside
/// an owned braid type.
#[derive(
    Clone,
    Debug,
    Hash,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize,
    facet::Facet,
)]
#[facet(transparent)]
pub struct MinimalString(String);

impl From<String> for MinimalString {
    fn from(s: String) -> Self {
        MinimalString(s)
    }
}

impl From<&'_ str> for MinimalString {
    fn from(s: &str) -> Self {
        MinimalString(s.into())
    }
}

impl From<Box<str>> for MinimalString {
    fn from(s: Box<str>) -> Self {
        MinimalString(s.into())
    }
}

impl AsRef<str> for MinimalString {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl From<MinimalString> for String {
    fn from(s: MinimalString) -> Self {
        s.0
    }
}
