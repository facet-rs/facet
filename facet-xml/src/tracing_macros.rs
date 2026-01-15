//! Tracing macros that compile to nothing when tracing is disabled.
//!
//! Tracing is enabled when either:
//! - The `tracing` feature is enabled (for production use)
//! - Running tests (`cfg(test)`) - tracing is always available in tests

/// Emit a trace-level log message.
#[cfg(any(test, feature = "tracing"))]
#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => {
        tracing::trace!($($arg)*);
    };
}

/// Emit a trace-level log message (no-op version).
#[cfg(not(any(test, feature = "tracing")))]
#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => {};
}
