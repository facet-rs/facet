// Zero-cost tracing macros for roam-shm
//
// These macros forward to tracing when the `tracing` feature is enabled,
// and compile to nothing when disabled.

// -----------------------------------------------------------------------------
// trace! - Very verbose: function entry/exit, loop iterations, hot paths
// -----------------------------------------------------------------------------

#[cfg(feature = "tracing")]
macro_rules! trace {
    ($($arg:tt)*) => { ::tracing::trace!($($arg)*) }
}

#[cfg(not(feature = "tracing"))]
macro_rules! trace {
    ($($arg:tt)*) => {};
}

// -----------------------------------------------------------------------------
// debug! - Intermediate values, decision points, useful for debugging
// -----------------------------------------------------------------------------

#[cfg(feature = "tracing")]
macro_rules! debug {
    ($($arg:tt)*) => { ::tracing::debug!($($arg)*) }
}

#[cfg(not(feature = "tracing"))]
macro_rules! debug {
    ($($arg:tt)*) => {};
}

// -----------------------------------------------------------------------------
// info! - High-level operations, usually for production logs
// -----------------------------------------------------------------------------

#[cfg(feature = "tracing")]
macro_rules! info {
    ($($arg:tt)*) => { ::tracing::info!($($arg)*) }
}

#[cfg(not(feature = "tracing"))]
macro_rules! info {
    ($($arg:tt)*) => {};
}

// -----------------------------------------------------------------------------
// warn! - Recoverable issues, things that might indicate problems
// -----------------------------------------------------------------------------

#[cfg(feature = "tracing")]
macro_rules! warn {
    ($($arg:tt)*) => { ::tracing::warn!($($arg)*) }
}

#[cfg(not(feature = "tracing"))]
macro_rules! warn {
    ($($arg:tt)*) => {};
}

// -----------------------------------------------------------------------------
// error! - Failures, things that went wrong
// -----------------------------------------------------------------------------

#[cfg(feature = "tracing")]
#[allow(unused_macros)]
macro_rules! error {
    ($($arg:tt)*) => { ::tracing::error!($($arg)*) }
}

#[cfg(not(feature = "tracing"))]
#[allow(unused_macros)]
macro_rules! error {
    ($($arg:tt)*) => {};
}

// Macros are made available via #[macro_use] on the module in lib.rs
