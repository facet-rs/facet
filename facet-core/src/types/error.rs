use super::{Ox, Shape, VTableErased};

/// Error returned by parse operations.
///
/// Can hold either a shaped error value (the actual error from `FromStr`)
/// or a static string message.
pub enum ParseError {
    /// A shaped error value (e.g., `ParseIntError`, `ParseBoolError`).
    /// The `Ox` owns the error and will drop it properly.
    Ox(Ox<'static>),
    /// A static string message for simple error cases.
    Str(&'static str),
}

impl ParseError {
    /// Create a ParseError from a shaped error value.
    pub fn from_error<E: crate::Facet<'static>>(error: E) -> Self {
        ParseError::Ox(Ox::new(error))
    }

    /// Create a ParseError from a static string.
    pub const fn from_str(msg: &'static str) -> Self {
        ParseError::Str(msg)
    }
}

impl core::fmt::Debug for ParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ParseError::Ox(ox) => {
                // Try to use the vtable's debug function
                let shape = ox.shape();
                match shape.vtable {
                    VTableErased::Direct(vt) => {
                        if let Some(debug_fn) = vt.debug {
                            // Safety: ox contains a valid value of the correct type
                            unsafe { debug_fn(ox.ptr_const().as_byte_ptr() as *const (), f) }
                        } else {
                            write!(f, "ParseError::Ox(<no Debug>)")
                        }
                    }
                    VTableErased::Indirect(vt) => {
                        if let Some(debug_fn) = vt.debug {
                            // Safety: ox contains a valid value of the correct type
                            unsafe { debug_fn(ox.as_ref().into(), f).unwrap_or(Ok(())) }
                        } else {
                            write!(f, "ParseError::Ox(<no Debug>)")
                        }
                    }
                }
            }
            ParseError::Str(msg) => write!(f, "ParseError::Str({msg:?})"),
        }
    }
}

impl core::fmt::Display for ParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ParseError::Ox(ox) => {
                // Try to use the vtable's display function
                let shape = ox.shape();
                match shape.vtable {
                    VTableErased::Direct(vt) => {
                        if let Some(display_fn) = vt.display {
                            // Safety: ox contains a valid value of the correct type
                            unsafe { display_fn(ox.ptr_const().as_byte_ptr() as *const (), f) }
                        } else {
                            write!(f, "parse error")
                        }
                    }
                    VTableErased::Indirect(vt) => {
                        if let Some(display_fn) = vt.display {
                            // Safety: ox contains a valid value of the correct type
                            unsafe { display_fn(ox.as_ref().into(), f).unwrap_or(Ok(())) }
                        } else {
                            write!(f, "parse error")
                        }
                    }
                }
            }
            ParseError::Str(msg) => write!(f, "{msg}"),
        }
    }
}

impl core::error::Error for ParseError {}

/// Outcome of a `try_from` vtable operation.
///
/// This enum encodes both the result and whether the source value was consumed,
/// which is critical for correct memory management.
#[derive(Debug, Clone)]
pub enum TryFromOutcome {
    /// Conversion succeeded. The source value was consumed.
    Converted,
    /// The source type is not supported by this converter.
    /// The source value was NOT consumed - caller retains ownership.
    Unsupported,
    /// Conversion failed after consuming the source.
    /// The source value WAS consumed - caller must not drop it.
    Failed(alloc::borrow::Cow<'static, str>),
}

/// Error returned when `try_from` fails to convert a value.
#[derive(Debug, Clone)]
pub enum TryFromError {
    /// The source shape is not supported for conversion.
    UnsupportedSourceShape {
        /// The shape of the source value.
        src_shape: &'static Shape,
        /// The expected shapes that would be supported.
        expected: &'static [&'static Shape],
    },
    /// The source type is not supported (simpler variant without detailed shape info).
    /// The source shape information is available in the enclosing error context.
    UnsupportedSourceType,
    /// A generic error message.
    Generic(alloc::string::String),
}

impl core::fmt::Display for TryFromError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            TryFromError::UnsupportedSourceShape {
                src_shape,
                expected,
            } => {
                write!(f, "unsupported source shape: {}", src_shape.type_identifier)?;
                if !expected.is_empty() {
                    write!(f, ", expected one of: ")?;
                    for (i, shape) in expected.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{}", shape.type_identifier)?;
                    }
                }
                Ok(())
            }
            TryFromError::UnsupportedSourceType => {
                write!(f, "unsupported source type for conversion")
            }
            TryFromError::Generic(msg) => write!(f, "{msg}"),
        }
    }
}

impl core::error::Error for TryFromError {}

/// Error returned when `try_into_inner` fails.
#[derive(Debug)]
pub enum TryIntoInnerError {
    /// The type does not support extracting an inner value.
    NotSupported,
    /// A generic error message.
    Generic(&'static str),
}

impl core::fmt::Display for TryIntoInnerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            TryIntoInnerError::NotSupported => write!(f, "try_into_inner not supported"),
            TryIntoInnerError::Generic(msg) => write!(f, "{msg}"),
        }
    }
}

impl core::error::Error for TryIntoInnerError {}

/// Error returned when `try_borrow_inner` fails.
#[derive(Debug)]
pub enum TryBorrowInnerError {
    /// The type does not support borrowing an inner value.
    NotSupported,
    /// A generic error message.
    Generic(&'static str),
}

impl core::fmt::Display for TryBorrowInnerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            TryBorrowInnerError::NotSupported => write!(f, "try_borrow_inner not supported"),
            TryBorrowInnerError::Generic(msg) => write!(f, "{msg}"),
        }
    }
}

impl core::error::Error for TryBorrowInnerError {}
