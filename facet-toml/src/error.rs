//! Errors from parsing TOML documents.

/// Any error
#[derive(Debug, Clone)]
pub struct AnyErr(pub(crate) String);

impl core::fmt::Display for AnyErr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for AnyErr {}

impl From<String> for AnyErr {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for AnyErr {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}
