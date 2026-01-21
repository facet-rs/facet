// Zero-cost tracing macros for facet-html-diff
//
// These macros forward to tracing when the `tracing` feature is enabled or in tests,
// and compile to nothing otherwise.

#[cfg(any(test, feature = "tracing"))]
macro_rules! trace {
    ($($arg:tt)*) => { ::tracing::trace!($($arg)*) }
}

#[cfg(not(any(test, feature = "tracing")))]
macro_rules! trace {
    ($($arg:tt)*) => {};
}

#[cfg(any(test, feature = "tracing"))]
macro_rules! debug {
    ($($arg:tt)*) => { ::tracing::debug!($($arg)*) }
}

#[cfg(not(any(test, feature = "tracing")))]
#[allow(unused_macros)]
macro_rules! debug {
    ($($arg:tt)*) => {};
}

#[allow(unused_imports)]
pub(crate) use trace;

#[allow(unused_imports)]
pub(crate) use debug;
