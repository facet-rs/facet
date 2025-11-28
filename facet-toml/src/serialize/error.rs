//! Errors from serializing TOML documents.

use alloc::string::String;

/// Any error from serializing TOML.
pub enum TomlSerError {
    /// Formatting error while writing TOML output.
    Fmt(core::fmt::Error),
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
    /// TOML doesn't support byte arrays.
    UnsupportedByteArray,
    /// Invalid array of tables (expected structs)
    InvalidArrayOfTables,
    /// TOML root must be a struct/table
    RootMustBeStruct,
    /// TOML doesn't support None/null values
    UnsupportedNone,
    /// TOML doesn't support unit type
    UnsupportedUnit,
    /// TOML doesn't support unit structs
    UnsupportedUnitStruct,
    /// Unsupported pointer type
    UnsupportedPointer,
    /// Unsupported type
    UnsupportedType {
        /// Name of the unsupported type
        type_name: String,
    },
    /// Unsupported scalar type
    UnsupportedScalarType {
        /// Name of the unsupported scalar type
        scalar_type: String,
    },
    /// Unknown scalar shape
    UnknownScalarShape {
        /// Description of the shape
        shape: String,
    },
}

impl From<core::fmt::Error> for TomlSerError {
    fn from(err: core::fmt::Error) -> Self {
        Self::Fmt(err)
    }
}

impl core::fmt::Display for TomlSerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Fmt(err) => write!(f, "Formatting error: {err}"),
            Self::InvalidNumberToI64Conversion { source_type } => {
                write!(f, "Error converting {source_type} to i64, out of range")
            }
            Self::InvalidKeyConversion { toml_type } => {
                write!(f, "Error converting type {toml_type} to TOML key")
            }
            Self::UnsupportedByteArray => {
                write!(f, "TOML doesn't support byte arrays")
            }
            Self::InvalidArrayOfTables => {
                write!(f, "Invalid array of tables: expected array of structs")
            }
            Self::RootMustBeStruct => {
                write!(f, "TOML root must be a struct/table")
            }
            Self::UnsupportedNone => {
                write!(f, "TOML doesn't support None/null values")
            }
            Self::UnsupportedUnit => {
                write!(f, "TOML doesn't support unit type")
            }
            Self::UnsupportedUnitStruct => {
                write!(f, "TOML doesn't support unit structs")
            }
            Self::UnsupportedPointer => {
                write!(f, "Unsupported pointer type in TOML serialization")
            }
            Self::UnsupportedType { type_name } => {
                write!(f, "Unsupported type for TOML serialization: {type_name}")
            }
            Self::UnsupportedScalarType { scalar_type } => {
                write!(
                    f,
                    "Unsupported scalar type for TOML serialization: {scalar_type}"
                )
            }
            Self::UnknownScalarShape { shape } => {
                write!(f, "Unknown scalar shape for TOML serialization: {shape}")
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
