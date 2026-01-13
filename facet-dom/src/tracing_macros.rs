//! Tracing macros that compile to nothing when the tracing feature is disabled.

/// Emit a trace-level log message.
macro_rules! trace {
    ($($arg:tt)*) => {
        #[cfg(feature = "tracing")]
        tracing::trace!($($arg)*);
    };
}

/// Enter a trace-level span.
macro_rules! trace_span {
    ($($arg:tt)*) => {
        #[cfg(feature = "tracing")]
        let _span = tracing::trace_span!($($arg)*).entered();
    };
}

pub(crate) use trace;
pub(crate) use trace_span;
