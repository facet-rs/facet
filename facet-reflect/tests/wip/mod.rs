#[cfg(all(feature = "std", feature = "slow-tests"))]
mod compile_tests;

#[cfg(feature = "std")]
mod no_uninit;

#[cfg(feature = "std")]
mod misc;

#[cfg(feature = "std")]
mod map;

#[cfg(feature = "std")]
mod list_leak;

#[cfg(feature = "std")]
mod map_leak;

#[cfg(feature = "std")]
mod invariant;

#[cfg(feature = "std")]
mod struct_leak;

#[cfg(feature = "std")]
mod put_vec_leak;

#[cfg(feature = "std")]
mod option_leak;

#[cfg(feature = "std")]
mod put_into_tuples;
