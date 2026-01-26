#![allow(unused_imports)]
#![allow(unused_macros)]

// Zero-cost tracing macros for figue
//
// These macros forward to tracing when the `tracing` feature is enabled,
// and compile to nothing when disabled.

// -----------------------------------------------------------------------------
// trace! - Very verbose: function entry/exit, loop iterations, hot paths
// -----------------------------------------------------------------------------

#[cfg(any(feature = "tracing", test))]
macro_rules! trace {
    ($($arg:tt)*) => { ::tracing::trace!($($arg)*) }
}

#[cfg(not(any(feature = "tracing", test)))]
macro_rules! trace {
    ($($arg:tt)*) => {};
}

// -----------------------------------------------------------------------------
// debug! - Intermediate values, decision points, useful for debugging
// -----------------------------------------------------------------------------

#[cfg(any(feature = "tracing", test))]
macro_rules! debug {
    ($($arg:tt)*) => { ::tracing::debug!($($arg)*) }
}

#[cfg(not(any(feature = "tracing", test)))]
macro_rules! debug {
    ($($arg:tt)*) => {};
}

// -----------------------------------------------------------------------------
// Make macros available throughout the crate
// -----------------------------------------------------------------------------

pub(crate) use debug;
pub(crate) use trace;
