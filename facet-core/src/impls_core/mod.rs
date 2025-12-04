mod array;

#[cfg(feature = "fn-ptr")]
mod fn_ptr;

mod nonnull;
mod ops;
mod option;
mod pointer;
mod reference;
mod result;
mod scalar;
mod slice;
mod tuple;

// Only include SIMD support when both the feature is enabled and we're on nightly
#[cfg(all(feature = "nightly", nightly))]
mod simd;
