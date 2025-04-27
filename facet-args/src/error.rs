use facet_reflect::ReflectError;

/// Error deserializing the Arguments
pub struct ArgsError {
    /// Type of error
    pub kind: ArgsErrorKind,
}

impl ArgsError {
    /// Create a new error.
    pub fn new(kind: ArgsErrorKind) -> Self {
        Self { kind }
    }
    /// The message for this specific error.
    pub fn message(&self) -> String {
        match &self.kind {
            ArgsErrorKind::GenericReflect(reflect_error) => {
                format!("Error while reflecting type: {reflect_error}")
            }
            ArgsErrorKind::GenericArgsError(message) => format!("Args error: {message}"),
        }
    }
}

impl core::fmt::Display for ArgsError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl core::fmt::Debug for ArgsError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Display::fmt(self, f)
    }
}

impl core::error::Error for ArgsError {}

/// Type of error.
#[derive(Debug, PartialEq)]
pub enum ArgsErrorKind {
    /// Any error from facet
    GenericReflect(ReflectError),
    /// Parsing arguments error
    GenericArgsError(String),
}
