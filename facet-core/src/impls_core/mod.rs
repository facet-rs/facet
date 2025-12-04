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

// Include SIMD support when portable_simd is available (detected via autocfg)
#[cfg(has_portable_simd)]
mod simd;
