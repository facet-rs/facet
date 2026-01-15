//! Tracing macros that compile to nothing when the `tracing` feature is disabled.

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
