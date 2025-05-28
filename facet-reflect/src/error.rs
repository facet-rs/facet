use facet_core::{Characteristic, EnumType, FieldError, Shape, TryFromError};
use owo_colors::OwoColorize;

/// Errors that can occur when reflecting on types.
#[derive(PartialEq, Clone)]
#[non_exhaustive]
pub enum ReflectError<'shape> {
    /// Tried to set an enum to a variant that does not exist
    NoSuchVariant {
        /// The enum definition containing all known variants.
        enum_type: EnumType<'shape>,
    },

    /// Tried to get the wrong shape out of a value — e.g. we were manipulating
    /// a `String`, but `.get()` was called with a `u64` or something.
    WrongShape {
        /// The expected shape of the value.
        expected: &'shape Shape<'shape>,
        /// The actual shape of the value.
        actual: &'shape Shape<'shape>,
    },

    /// Attempted to perform an operation that expected a struct or something
    WasNotA {
        /// The name of the expected type.
        expected: &'shape str,

        /// The type we got instead
        actual: &'shape Shape<'shape>,
    },

    /// A field was not initialized during build
    UninitializedField {
        /// The shape containing the field
        shape: &'shape Shape<'shape>,
        /// The name of the field that wasn't initialized
        field_name: &'shape str,
    },

    /// A field in an enum variant was not initialized during build
    UninitializedEnumField {
        /// The enum shape
        shape: &'shape Shape<'shape>,
        /// The name of the field that wasn't initialized
        field_name: &'shape str,
        /// The name of the variant containing the field
        variant_name: &'shape str,
    },

    /// A scalar value was not initialized during build
    UninitializedValue {
        /// The scalar shape
        shape: &'shape Shape<'shape>,
    },

    /// An invariant of the reflection system was violated.
    InvariantViolation {
        /// The invariant that was violated.
        invariant: &'shape str,
    },

    /// Attempted to set a value to its default, but the value doesn't implement `Default`.
    MissingCharacteristic {
        /// The shape of the value that doesn't implement `Default`.
        shape: &'shape Shape<'shape>,
        /// The characteristic that is missing.
        characteristic: Characteristic,
    },

    /// An operation failed for a given shape
    OperationFailed {
        /// The shape of the value for which the operation failed.
        shape: &'shape Shape<'shape>,
        /// The name of the operation that failed.
        operation: &'shape str,
    },

    /// An error occurred when attempting to access or modify a field.
    FieldError {
        /// The shape of the value containing the field.
        shape: &'shape Shape<'shape>,
        /// The specific error that occurred with the field.
        field_error: FieldError,
    },

    /// Indicates that we try to access a field on an `Arc<T>`, for example, and the field might exist
    /// on the T, but you need to do push_pointee first when using the WIP API.
    MissingPushPointee {
        /// The smart pointer (`Arc<T>`, `Box<T>` etc.) shape on which field was caleld
        shape: &'shape Shape<'shape>,
    },

    /// An unknown error occurred.
    Unknown,

    /// An error occured while putting
    TryFromError {
        /// The shape of the value being converted from.
        src_shape: &'shape Shape<'shape>,

        /// The shape of the value being converted to.
        dst_shape: &'shape Shape<'shape>,

        /// The inner error
        inner: TryFromError<'shape>,
    },

    /// A shape has a `default` attribute, but no implementation of the `Default` trait.
    DefaultAttrButNoDefaultImpl {
        /// The shape of the value that has a `default` attribute but no default implementation.
        shape: &'shape Shape<'shape>,
    },

    /// The type is unsized
    Unsized {
        /// The shape for the type that is unsized
        shape: &'shape Shape<'shape>,
    },

    /// Array not fully initialized during build
    ArrayNotFullyInitialized {
        /// The shape of the array
        shape: &'shape Shape<'shape>,
        /// The number of elements pushed
        pushed_count: usize,
        /// The expected array size
        expected_size: usize,
    },

    /// Array index out of bounds
    ArrayIndexOutOfBounds {
        /// The shape of the array
        shape: &'shape Shape<'shape>,
        /// The index that was out of bounds
        index: usize,
        /// The array size
        size: usize,
    },
}

impl core::fmt::Display for ReflectError<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ReflectError::NoSuchVariant { enum_type } => {
                write!(f, "No such variant in enum. Known variants: ")?;
                for v in enum_type.variants {
                    write!(f, ", {}", v.name.cyan())?;
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
                    shape.blue(),
                    field_name.yellow(),
                    variant_name.red()
                )
            }
            ReflectError::UninitializedValue { shape } => {
                write!(f, "Value '{}' was not initialized", shape.blue())
            }
            ReflectError::InvariantViolation { invariant } => {
                write!(f, "Invariant violation: {}", invariant.red())
            }
            ReflectError::MissingCharacteristic {
                shape,
                characteristic,
            } => write!(
                f,
                "{shape} does not implement characteristic {characteristic:?}",
            ),
            ReflectError::OperationFailed { shape, operation } => {
                write!(
                    f,
                    "Operation failed on shape {}: {}",
                    shape.blue(),
                    operation
                )
            }
            ReflectError::FieldError { shape, field_error } => {
                write!(f, "Field error for shape {}: {}", shape.red(), field_error)
            }
            ReflectError::MissingPushPointee { shape } => {
                write!(
                    f,
                    "Tried to access a field on smart pointer '{}', but you need to call {} first to work with the value it points to (and pop it with {} later)",
                    shape.blue(),
                    ".push_pointee()".yellow(),
                    ".pop()".yellow()
                )
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
                shape.red()
            ),
            ReflectError::Unsized { shape } => write!(f, "Shape '{}' is unsized", shape.red()),
            ReflectError::ArrayNotFullyInitialized {
                shape,
                pushed_count,
                expected_size,
            } => {
                write!(
                    f,
                    "Array '{}' not fully initialized: expected {} elements, but got {}",
                    shape.blue(),
                    expected_size,
                    pushed_count
                )
            }
            ReflectError::ArrayIndexOutOfBounds { shape, index, size } => {
                write!(
                    f,
                    "Array index {} out of bounds for '{}' (array length is {})",
                    index,
                    shape.blue(),
                    size
                )
            }
        }
    }
}

impl core::fmt::Debug for ReflectError<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Use Display implementation for more readable output
        write!(f, "ReflectError({})", self)
    }
}

impl core::error::Error for ReflectError<'_> {}
