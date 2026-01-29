use facet_core::{Characteristic, EnumType, FieldError, Shape, TryFromError};
use facet_path::Path;

/// Error returned when materializing a HeapValue to the wrong type.
///
/// This is separate from `ReflectError` because HeapValue operations
/// don't have path context - they operate on already-constructed values.
#[derive(Debug, Clone)]
pub struct ShapeMismatchError {
    /// The shape that was expected (the target type).
    pub expected: &'static Shape,
    /// The shape that was actually found (the HeapValue's shape).
    pub actual: &'static Shape,
}

impl core::fmt::Display for ShapeMismatchError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "shape mismatch: expected {}, got {}",
            self.expected, self.actual
        )
    }
}

impl core::error::Error for ShapeMismatchError {}

/// Error returned when allocating memory for a shape fails.
///
/// This is separate from `ReflectError` because allocation happens
/// before reflection begins - we don't have a path yet.
#[derive(Debug, Clone)]
pub struct AllocError {
    /// The shape we tried to allocate.
    pub shape: &'static Shape,
    /// What operation was being attempted.
    pub operation: &'static str,
}

impl core::fmt::Display for AllocError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "failed to allocate {}: {}", self.shape, self.operation)
    }
}

impl core::error::Error for AllocError {}

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

/// Error that occurred during reflection, with path context.
#[derive(Clone)]
pub struct ReflectError {
    /// Path through the type structure where the error occurred.
    pub path: Path,

    /// The specific kind of error.
    pub kind: ReflectErrorKind,
}

impl ReflectError {
    /// Create a new ReflectError with path context.
    #[inline]
    pub fn new(kind: ReflectErrorKind, path: Path) -> Self {
        Self { path, kind }
    }
}

/// Specific kinds of reflection errors.
#[derive(Clone)]
#[non_exhaustive]
pub enum ReflectErrorKind {
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

    /// A scalar value was not initialized during build
    UninitializedValue {
        /// The scalar shape
        shape: &'static Shape,
    },

    /// A field validation failed
    ValidationFailed {
        /// The shape containing the field
        shape: &'static Shape,
        /// The name of the field that failed validation
        field_name: &'static str,
        /// The validation error message
        message: alloc::string::String,
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

    /// Failed to parse a string value into the target type
    ParseFailed {
        /// The shape we were trying to parse into.
        shape: &'static Shape,
        /// The input string that failed to parse.
        input: alloc::string::String,
    },

    /// An error occurred when attempting to access or modify a field.
    FieldError {
        /// The shape of the value containing the field.
        shape: &'static Shape,
        /// The specific error that occurred with the field.
        field_error: FieldError,
    },

    /// Attempted to mutate struct fields on a type that is not POD (Plain Old Data).
    ///
    /// Field mutation through reflection requires the parent struct to be POD
    /// (have no invariants). Mark the struct with `#[facet(pod)]` to enable
    /// field mutation. Wholesale replacement via `Poke::set()` is always allowed.
    NotPod {
        /// The shape of the struct that is not POD.
        shape: &'static Shape,
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
        write!(f, "{} at {}", self.kind, self.path)
    }
}

impl core::fmt::Display for ReflectErrorKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ReflectErrorKind::NoSuchVariant { enum_type } => {
                write!(f, "No such variant in enum. Known variants: ")?;
                for v in enum_type.variants {
                    write!(f, ", {}", v.name)?;
                }
                write!(f, ", that's it.")
            }
            ReflectErrorKind::WrongShape { expected, actual } => {
                write!(f, "Wrong shape: expected {expected}, but got {actual}")
            }
            ReflectErrorKind::WasNotA { expected, actual } => {
                write!(f, "Wrong shape: expected {expected}, but got {actual}")
            }
            ReflectErrorKind::UninitializedField { shape, field_name } => {
                write!(
                    f,
                    "Field '{shape}::{field_name}' was not initialized. \
                    If you need to leave fields partially initialized and come back later, \
                    use deferred mode (begin_deferred/finish_deferred)"
                )
            }
            ReflectErrorKind::UninitializedValue { shape } => {
                write!(
                    f,
                    "Value '{shape}' was not initialized. \
                    If you need to leave values partially initialized and come back later, \
                    use deferred mode (begin_deferred/finish_deferred)"
                )
            }
            ReflectErrorKind::ValidationFailed {
                shape,
                field_name,
                message,
            } => {
                write!(
                    f,
                    "Validation failed for field '{shape}::{field_name}': {message}"
                )
            }
            ReflectErrorKind::InvariantViolation { invariant } => {
                write!(f, "Invariant violation: {invariant}")
            }
            ReflectErrorKind::MissingCharacteristic {
                shape,
                characteristic,
            } => write!(
                f,
                "{shape} does not implement characteristic {characteristic:?}",
            ),
            ReflectErrorKind::OperationFailed { shape, operation } => {
                write!(f, "Operation failed on shape {shape}: {operation}")
            }
            ReflectErrorKind::ParseFailed { shape, input } => {
                write!(f, "failed to parse \"{input}\" as {shape}")
            }
            ReflectErrorKind::FieldError { shape, field_error } => {
                write!(f, "Field error for shape {shape}: {field_error}")
            }
            ReflectErrorKind::NotPod { shape } => {
                write!(
                    f,
                    "Cannot mutate fields of '{shape}' - it is not POD (Plain Old Data). \
                     Add #[facet(pod)] to the struct to enable field mutation. \
                     (Wholesale replacement via Poke::set() is always allowed.)"
                )
            }
            ReflectErrorKind::MissingPushPointee { shape } => {
                write!(
                    f,
                    "Tried to access a field on smart pointer '{shape}', but you need to call .begin_smart_ptr() first to work with the value it points to (and pop it with .pop() later)"
                )
            }
            ReflectErrorKind::Unknown => write!(f, "Unknown error"),
            ReflectErrorKind::TryFromError {
                src_shape,
                dst_shape,
                inner,
            } => {
                write!(
                    f,
                    "While trying to put {src_shape} into a {dst_shape}: {inner}"
                )
            }
            ReflectErrorKind::DefaultAttrButNoDefaultImpl { shape } => write!(
                f,
                "Shape '{shape}' has a `default` attribute but no default implementation"
            ),
            ReflectErrorKind::Unsized { shape, operation } => write!(
                f,
                "Shape '{shape}' is unsized, can't perform operation {operation}"
            ),
            ReflectErrorKind::ArrayNotFullyInitialized {
                shape,
                pushed_count,
                expected_size,
            } => {
                write!(
                    f,
                    "Array '{shape}' not fully initialized: expected {expected_size} elements, but got {pushed_count}"
                )
            }
            ReflectErrorKind::ArrayIndexOutOfBounds { shape, index, size } => {
                write!(
                    f,
                    "Array index {index} out of bounds for '{shape}' (array length is {size})"
                )
            }
            ReflectErrorKind::InvalidOperation { operation, reason } => {
                write!(f, "Invalid operation '{operation}': {reason}")
            }
            ReflectErrorKind::UnexpectedTracker {
                message,
                current_tracker,
            } => {
                write!(f, "{message}: current tracker is {current_tracker:?}")
            }
            ReflectErrorKind::NoActiveFrame => {
                write!(f, "No active frame in Partial")
            }
            #[cfg(feature = "alloc")]
            ReflectErrorKind::CustomDeserializationError {
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
            ReflectErrorKind::CustomSerializationError {
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
            ReflectErrorKind::UserInvariantFailed { message, shape } => {
                write!(f, "Invariant check failed for '{shape}': {message}")
            }
        }
    }
}

impl core::fmt::Debug for ReflectError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ReflectError")
            .field("path", &self.path)
            .field("kind", &self.kind)
            .finish()
    }
}

impl core::fmt::Debug for ReflectErrorKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Use Display implementation for more readable output
        write!(f, "ReflectErrorKind({self})")
    }
}

impl core::error::Error for ReflectError {}
impl core::error::Error for ReflectErrorKind {}

impl From<AllocError> for ReflectError {
    fn from(e: AllocError) -> Self {
        ReflectError {
            path: Path::new(e.shape),
            kind: ReflectErrorKind::OperationFailed {
                shape: e.shape,
                operation: e.operation,
            },
        }
    }
}
