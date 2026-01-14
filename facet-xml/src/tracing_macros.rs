//! Tracing macros that compile to nothing when tracing is disabled.
//!
//! Tracing is enabled when either:
//! - The `tracing` feature is enabled (for production use)
//! - Running tests (`cfg(test)`) - tracing is always available in tests

/// Emit a trace-level log message.
#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => {
        #[cfg(any(test, feature = "tracing"))]
        tracing::trace!($($arg)*);
    };
}
