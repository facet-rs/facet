#![cfg(feature = "std")]

#[cfg(feature = "slow-tests")]
mod compile_tests;

mod no_uninit;

mod misc;

mod map;

mod list_leak;

mod map_leak;

mod invariant;

mod struct_leak;

mod put_vec_leak;

mod option_leak;

mod put_into_tuples;

mod variance;
