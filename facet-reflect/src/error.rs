use facet_core::{Characteristic, EnumType, FieldError, Shape, TryFromError};

/// A kind-only version of Tracker
#[allow(missing_docs)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[non_exhaustive]
pub enum TrackerKind {
    Scalar,
    Array,
    Struct,
    SmartPointer,
    SmartPointerSlice,
    Enum,
    List,
    Map,
    Set,
    Option,
    Result,
    DynamicValue,
}

/// Errors that can occur when reflecting on types.
#[derive(Clone)]
pub enum ReflectError {
    /// Tried to set an enum to a variant that does not exist
    NoSuchVariant {
        /// The enum definition containing all known variants.
        enum_type: EnumType,
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

    /// Indicates that we try to access a field on an `Arc<T>`, for example, and the field might exist
    /// on the T, but you need to do begin_smart_ptr first when using the WIP API.
    MissingPushPointee {
        /// The smart pointer (`Arc<T>`, `Box<T>` etc.) shape on which field was caleld
        shape: &'static Shape,
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

    /// The type is unsized
    Unsized {
        /// The shape for the type that is unsized
        shape: &'static Shape,
        /// The operation we were trying to perform
        operation: &'static str,
    },

    /// Array not fully initialized during build
    ArrayNotFullyInitialized {
        /// The shape of the array
        shape: &'static Shape,
        /// The number of elements pushed
        pushed_count: usize,
        /// The expected array size
        expected_size: usize,
    },

    /// Array index out of bounds
    ArrayIndexOutOfBounds {
        /// The shape of the array
        shape: &'static Shape,
        /// The index that was out of bounds
        index: usize,
        /// The array size
        size: usize,
    },

    /// Invalid operation for the current state
    InvalidOperation {
        /// The operation that was attempted
        operation: &'static str,
        /// The reason why it failed
        reason: &'static str,
    },

    /// Unexpected tracker state when performing a reflection operation
    UnexpectedTracker {
        /// User-friendly message including operation that was being
        /// attempted
        message: &'static str,

        /// The current tracker set for this frame
        current_tracker: TrackerKind,
    },

    /// No active frame in Partial
    NoActiveFrame,

    #[cfg(feature = "alloc")]
    /// Error during custom deserialization
    CustomDeserializationError {
        /// Error message provided by the deserialize_with method
        message: alloc::string::String,
        /// Shape that was passed to deserialize_with
        src_shape: &'static Shape,
        /// the shape of the target type
        dst_shape: &'static Shape,
    },

    #[cfg(feature = "alloc")]
    /// A user-defined invariant check failed during build
    UserInvariantFailed {
        /// The error message from the invariant check
        message: alloc::string::String,
        /// The shape of the value that failed the invariant check
        shape: &'static Shape,
    },

    #[cfg(feature = "alloc")]
    /// Error during custom serialization
    CustomSerializationError {
        /// Error message provided by the serialize_with method
        message: alloc::string::String,
        /// Shape that was passed to serialize_with
        src_shape: &'static Shape,
        /// the shape of the target
        dst_shape: &'static Shape,
    },
}

impl core::fmt::Display for ReflectError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ReflectError::NoSuchVariant { enum_type } => {
                write!(f, "No such variant in enum. Known variants: ")?;
                for v in enum_type.variants {
                    write!(f, ", {}", v.name)?;
                }
                write!(f, ", that's it.")
            }
            ReflectError::WrongShape { expected, actual } => {
                write!(f, "Wrong shape: expected {expected}, but got {actual}")
            }
            ReflectError::WasNotA { expected, actual } => {
                write!(f, "Wrong shape: expected {expected}, but got {actual}")
            }
            ReflectError::UninitializedField { shape, field_name } => {
                write!(
                    f,
                    "Field '{shape}::{field_name}' was not initialized. \
                    If you need to leave fields partially initialized and come back later, \
                    use deferred mode (begin_deferred/finish_deferred)"
                )
            }
            ReflectError::UninitializedEnumField {
                shape,
                field_name,
                variant_name,
            } => {
                write!(
                    f,
                    "Field '{shape}::{field_name}' in variant '{variant_name}' was not initialized. \
                    If you need to leave fields partially initialized and come back later, \
                    use deferred mode (begin_deferred/finish_deferred)"
                )
            }
            ReflectError::UninitializedValue { shape } => {
                write!(
                    f,
                    "Value '{shape}' was not initialized. \
                    If you need to leave values partially initialized and come back later, \
                    use deferred mode (begin_deferred/finish_deferred)"
                )
            }
            ReflectError::InvariantViolation { invariant } => {
                write!(f, "Invariant violation: {invariant}")
            }
            ReflectError::MissingCharacteristic {
                shape,
                characteristic,
            } => write!(
                f,
                "{shape} does not implement characteristic {characteristic:?}",
            ),
            ReflectError::OperationFailed { shape, operation } => {
                write!(f, "Operation failed on shape {shape}: {operation}")
            }
            ReflectError::FieldError { shape, field_error } => {
                write!(f, "Field error for shape {shape}: {field_error}")
            }
            ReflectError::MissingPushPointee { shape } => {
                write!(
                    f,
                    "Tried to access a field on smart pointer '{shape}', but you need to call .begin_smart_ptr() first to work with the value it points to (and pop it with .pop() later)"
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
                    "While trying to put {src_shape} into a {dst_shape}: {inner}"
                )
            }
            ReflectError::DefaultAttrButNoDefaultImpl { shape } => write!(
                f,
                "Shape '{shape}' has a `default` attribute but no default implementation"
            ),
            ReflectError::Unsized { shape, operation } => write!(
                f,
                "Shape '{shape}' is unsized, can't perform operation {operation}"
            ),
            ReflectError::ArrayNotFullyInitialized {
                shape,
                pushed_count,
                expected_size,
            } => {
                write!(
                    f,
                    "Array '{shape}' not fully initialized: expected {expected_size} elements, but got {pushed_count}"
                )
            }
            ReflectError::ArrayIndexOutOfBounds { shape, index, size } => {
                write!(
                    f,
                    "Array index {index} out of bounds for '{shape}' (array length is {size})"
                )
            }
            ReflectError::InvalidOperation { operation, reason } => {
                write!(f, "Invalid operation '{operation}': {reason}")
            }
            ReflectError::UnexpectedTracker {
                message,
                current_tracker,
            } => {
                write!(f, "{message}: current tracker is {current_tracker:?}")
            }
            ReflectError::NoActiveFrame => {
                write!(f, "No active frame in Partial")
            }
            #[cfg(feature = "alloc")]
            ReflectError::CustomDeserializationError {
                message,
                src_shape,
                dst_shape,
            } => {
                write!(
                    f,
                    "Custom deserialization of shape '{src_shape}' into '{dst_shape}' failed: {message}"
                )
            }
            #[cfg(feature = "alloc")]
            ReflectError::CustomSerializationError {
                message,
                src_shape,
                dst_shape,
            } => {
                write!(
                    f,
                    "Custom serialization of shape '{src_shape}' into '{dst_shape}' failed: {message}"
                )
            }
            #[cfg(feature = "alloc")]
            ReflectError::UserInvariantFailed { message, shape } => {
                write!(f, "Invariant check failed for '{shape}': {message}")
            }
        }
    }
}

impl core::fmt::Debug for ReflectError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Use Display implementation for more readable output
        write!(f, "ReflectError({self})")
    }
}

impl core::error::Error for ReflectError {}
