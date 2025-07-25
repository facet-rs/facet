//! Errors from parsing into YAML documents.

/// Any error from serializing YAML.
pub enum YamlSerError {
    /// Could not convert number to i64 representation.
    InvalidNumberToI64Conversion {
        /// Type of the number that's trying to be converted.
        source_type: &'static str,
    },
    /// Could not convert type to valid YAML key.
    InvalidKeyConversion {
        /// Type of the YAML value that's trying to be converted to a key.
        yaml_type: &'static str,
    },
    /// YAML doesn't support byte arrays.
    UnsupportedByteArray,
}

impl core::fmt::Display for YamlSerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidNumberToI64Conversion { source_type } => {
                write!(f, "Error converting {source_type} to i64, out of range")
            }
            Self::InvalidKeyConversion { yaml_type } => {
                write!(f, "Error converting type {yaml_type} to YAML key")
            }
            Self::UnsupportedByteArray => {
                write!(f, "YAML doesn't support byte arrays")
            }
        }
    }
}

impl core::error::Error for YamlSerError {}

impl core::fmt::Debug for YamlSerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Display::fmt(self, f)
    }
}
