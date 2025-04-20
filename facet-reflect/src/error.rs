use facet_core::{Characteristic, EnumDef, Field, FieldError, Shape, TryFromError};
use owo_colors::OwoColorize;

/// Errors that can occur when reflecting on types.
#[derive(Debug, PartialEq, Clone)]
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

    /// Attempted to perform an operation that expected a struct or something
    WasNotA {
        /// The name of the expected type.
        expected: &'static str,

        /// The type we got instead
        actual: &'static Shape,
    },

    /// A field was not initialized during build
    UninitializedField {
        /// The shape containing the field
        shape: &'static Shape,
        /// The name of the field that wasn't initialized
        field_name: &'static str,
    },

    /// A field in an enum variant was not initialized during build
    UninitializedEnumField {
        /// The enum shape
        shape: &'static Shape,
        /// The name of the field that wasn't initialized
        field_name: &'static str,
        /// The name of the variant containing the field
        variant_name: &'static str,
    },

    /// An enum had no variant selected during build
    NoVariantSelected {
        /// The enum shape
        shape: &'static Shape,
    },

    /// A scalar value was not initialized during build
    UninitializedValue {
        /// The scalar shape
        shape: &'static Shape,
    },

    /// An invariant of the reflection system was violated.
    InvariantViolation {
        /// The invariant that was violated.
        invariant: &'static str,
    },

    /// Attempted to set a value to its default, but the value doesn't implement `Default`.
    MissingCharacteristic {
        /// The shape of the value that doesn't implement `Default`.
        shape: &'static Shape,
        /// The characteristic that is missing.
        characteristic: Characteristic,
    },

    /// An operation failed for a given shape
    OperationFailed {
        /// The shape of the value for which the operation failed.
        shape: &'static Shape,
        /// The name of the operation that failed.
        operation: &'static str,
    },

    /// An error occurred when attempting to access or modify a field.
    FieldError {
        /// The shape of the value containing the field.
        shape: &'static Shape,
        /// The specific error that occurred with the field.
        field_error: FieldError,
    },

    /// An unknown error occurred.
    Unknown,

    /// An error occured while putting
    TryFromError {
        /// The shape of the value being converted from.
        src_shape: &'static Shape,

        /// The shape of the value being converted to.
        dst_shape: &'static Shape,

        /// The inner error
        inner: TryFromError,
    },

    /// A shape has a `default` attribute, but no implementation of the `Default` trait.
    DefaultAttrButNoDefaultImpl {
        /// The shape of the value that has a `default` attribute but no default implementation.
        shape: &'static Shape,
    },
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
                write!(
                    f,
                    "Wrong shape: expected {}, but got {}",
                    expected.green(),
                    actual.red()
                )
            }
            ReflectError::WasNotA { expected, actual } => {
                write!(
                    f,
                    "Wrong shape: expected {}, but got {}",
                    expected.green(),
                    actual.red()
                )
            }
            ReflectError::UninitializedField { shape, field_name } => {
                write!(f, "Field '{}::{}' was not initialized", shape, field_name)
            }
            ReflectError::UninitializedEnumField {
                shape,
                field_name,
                variant_name,
            } => {
                write!(
                    f,
                    "Field '{}::{}' in variant '{}' was not initialized",
                    shape, field_name, variant_name
                )
            }
            ReflectError::NoVariantSelected { shape } => {
                write!(f, "Enum '{}' had no variant selected", shape)
            }
            ReflectError::UninitializedValue { shape } => {
                write!(f, "Value '{}' was not initialized", shape)
            }
            ReflectError::InvariantViolation { invariant } => {
                write!(f, "Invariant violation: {}", invariant)
            }
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
            ReflectError::FieldError { shape, field_error } => {
                write!(f, "Field error for shape {}: {}", shape, field_error)
            }
            ReflectError::Unknown => write!(f, "Unknown error"),
            ReflectError::TryFromError {
                src_shape,
                dst_shape,
                inner,
            } => {
                write!(
                    f,
                    "While trying to put {} into a {}: {}",
                    src_shape.green(),
                    dst_shape.blue(),
                    inner.red()
                )
            }
            ReflectError::DefaultAttrButNoDefaultImpl { shape } => write!(
                f,
                "Shape '{}' has a `default` attribute but no default implementation",
                shape
            ),
        }
    }
}

impl core::error::Error for ReflectError {}
