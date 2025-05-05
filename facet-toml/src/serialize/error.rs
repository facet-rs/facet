//! Errors from parsing TOML documents.

/// Any error from serializing TOML.
pub enum TomlSerError {
    /// Could not convert number to i64 representation.
    InvalidNumberToI64Conversion {
        /// Type of the number that's trying to be converted.
        source_type: &'static str,
    },
    /// Could not convert type to valid TOML key.
    InvalidKeyConversion {
        /// Type of the TOML value that's trying to be converted to a key.
        toml_type: &'static str,
    },
}

impl core::fmt::Display for TomlSerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidNumberToI64Conversion { source_type } => {
                write!(f, "Error converting {source_type} to i64, out of range")
            }
            Self::InvalidKeyConversion { toml_type } => {
                write!(f, "Error converting type {toml_type} to TOML key")
            }
        }
    }
}

impl core::error::Error for TomlSerError {}

impl core::fmt::Debug for TomlSerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Display::fmt(self, f)
    }
}
