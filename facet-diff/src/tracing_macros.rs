//! Tracing macros that compile to nothing when tracing is disabled.
//!
//! Tracing is enabled when either:
//! - The `tracing` feature is enabled (for production use)
//! - Running tests (`cfg(test)`) - tracing is always available in tests

/// Emit an extremely verbose trace-level log message.
/// This is for tracing that's too noisy even for trace level.
/// Uncomment the forwarding to enable.
#[macro_export]
macro_rules! trace_verbose {
    ($($arg:tt)*) => {
        // tracing::trace!($($arg)*);
    };
}

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

/// Emit a debug-level log message.
#[cfg(any(test, feature = "tracing"))]
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        tracing::debug!($($arg)*);
    };
}

/// Emit a debug-level log message (no-op version).
#[cfg(not(any(test, feature = "tracing")))]
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {};
}
