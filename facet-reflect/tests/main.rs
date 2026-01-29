//! Consolidated integration tests for facet-reflect.

mod partial;
mod peek;
mod poke;

#[cfg(all(not(miri), feature = "slow-tests"))]
mod compile_tests;
