use facet_core::{Characteristic, EnumDef, Field, Shape};

/// Errors that can occur when reflecting on types.
#[derive(Debug)]
#[non_exhaustive]
pub enum ReflectError {
    /// Tried to `build` or `build_in_place` a struct/enum without initializing all fields.
    PartiallyInitialized {
        /// The field that was not initialized.
        field: Field,
    },

    /// Tried to set an enum to a variant that does not exist
    NoSuchVariant {
        /// The enum definition containing all known variants.
        enum_def: EnumDef,
    },

    /// Tried to get the wrong shape out of a value — e.g. we were manipulating
    /// a `String`, but `.get()` was called with a `u64` or something.
    WrongShape {
        /// The expected shape of the value.
        expected: &'static Shape,
        /// The actual shape of the value.
        actual: &'static Shape,
    },

    /// Attempted to perform an operation that expected a struct on a non-struct value.
    WasNotA {
        /// The name of the expected type.
        name: &'static str,
    },

    /// An invariant of the reflection system was violated.
    InvariantViolation,

    /// Attempted to set a value to its default, but the value doesn't implement `Default`.
    MissingCharacteristic {
        /// The shape of the value that doesn't implement `Default`.
        shape: &'static Shape,
        characteristic: Characteristic,
    },

    /// An operation failed for a given shape
    OperationFailed {
        shape: &'static Shape,
        operation: &'static str,
    },

    Unknown,
}

impl core::fmt::Display for ReflectError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ReflectError::PartiallyInitialized { field } => {
                write!(
                    f,
                    "Value partially initialized: field {} was not set",
                    field.name
                )
            }
            ReflectError::NoSuchVariant { enum_def } => {
                write!(f, "No such variant in enum. Known variants: ")?;
                for v in enum_def.variants {
                    write!(f, ", {}", v.name)?;
                }
                write!(f, ", that's it.")
            }
            ReflectError::WrongShape { expected, actual } => {
                write!(f, "Wrong shape: expected {}, but got {}", expected, actual)
            }
            ReflectError::WasNotA { name } => write!(f, "Was not a {}", name),
            ReflectError::InvariantViolation => write!(f, "Invariant violation"),
            ReflectError::MissingCharacteristic {
                shape,
                characteristic,
            } => write!(
                f,
                "{shape} does not implement characteristic {characteristic:?}",
            ),
            ReflectError::OperationFailed { shape, operation } => {
                write!(f, "Operation '{}' failed for shape {}", operation, shape)
            }
            ReflectError::Unknown => write!(f, "Unknown error"),
        }
    }
}

impl core::error::Error for ReflectError {}
