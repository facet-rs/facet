// Tracing macros for roam-shm
//
// These macros forward to the tracing crate.

// -----------------------------------------------------------------------------
// trace! - Very verbose: function entry/exit, loop iterations, hot paths
// -----------------------------------------------------------------------------

#![allow(unused_macro_rules)]

macro_rules! trace {
    ($($arg:tt)*) => { ::tracing::trace!($($arg)*) }
}

// -----------------------------------------------------------------------------
// debug! - Intermediate values, decision points, useful for debugging
// -----------------------------------------------------------------------------

macro_rules! debug {
    ($($arg:tt)*) => { ::tracing::debug!($($arg)*) }
}

// -----------------------------------------------------------------------------
// info! - High-level operations, usually for production logs
// -----------------------------------------------------------------------------

macro_rules! info {
    ($($arg:tt)*) => { ::tracing::info!($($arg)*) }
}

// -----------------------------------------------------------------------------
// warn! - Recoverable issues, things that might indicate problems
// -----------------------------------------------------------------------------

macro_rules! warn {
    ($($arg:tt)*) => { ::tracing::warn!($($arg)*) }
}

// -----------------------------------------------------------------------------
// error! - Failures, things that went wrong
// -----------------------------------------------------------------------------

#[allow(unused_macros)]
macro_rules! error {
    ($($arg:tt)*) => { ::tracing::error!($($arg)*) }
}

// Macros are made available via #[macro_use] on the module in lib.rs
