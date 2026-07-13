//! Inactive native backend surface for builds where Weavy JIT is unavailable.

use core::hash::Hasher;
use core::marker::PhantomData;

use facet_core::Facet;

use crate::HashError;

/// Native copy-and-patch hash plan stub.
pub struct NativeHashPlan<T, H = std::collections::hash_map::DefaultHasher> {
    _marker: PhantomData<fn() -> (T, H)>,
}

/// Native copy-and-patch equality plan stub.
pub struct NativeEqualityPlan<T> {
    _marker: PhantomData<fn() -> T>,
}

// SAFETY: inactive plans contain no executable memory or shared mutable state.
unsafe impl<T, H> Send for NativeHashPlan<T, H> {}
// SAFETY: inactive plans contain no executable memory or shared mutable state.
unsafe impl<T, H> Sync for NativeHashPlan<T, H> {}

// SAFETY: inactive plans contain no executable memory or shared mutable state.
unsafe impl<T> Send for NativeEqualityPlan<T> {}
// SAFETY: inactive plans contain no executable memory or shared mutable state.
unsafe impl<T> Sync for NativeEqualityPlan<T> {}

impl<T, H> NativeHashPlan<T, H>
where
    T: Facet<'static>,
    H: Hasher,
{
    /// Native hashing is unavailable in this build.
    pub fn build() -> Result<Self, HashError> {
        Err(HashError::Unsupported {
            shape: T::SHAPE,
            feature: "native hash JIT inactive",
        })
    }

    /// This inactive backend never hashes through native code.
    pub fn hash(&self, _value: &T, _hasher: &mut H) -> Result<(), HashError> {
        unreachable!("facet-hash native JIT is inactive for this build")
    }

    /// Return zero code-layout counters for the inactive backend.
    #[must_use]
    pub fn stats(&self) -> NativeHashPlanStats {
        NativeHashPlanStats::default()
    }
}

impl<T> NativeHashPlan<T, std::collections::hash_map::DefaultHasher>
where
    T: Facet<'static>,
{
    /// This inactive backend never hashes through native code.
    pub fn hash64(&self, _value: &T) -> Result<u64, HashError> {
        unreachable!("facet-hash native JIT is inactive for this build")
    }
}

impl<T> NativeEqualityPlan<T>
where
    T: Facet<'static>,
{
    /// Native equality is unavailable in this build.
    pub fn build() -> Result<Self, HashError> {
        Err(HashError::Unsupported {
            shape: T::SHAPE,
            feature: "native equality JIT inactive",
        })
    }

    /// This inactive backend never compares through native code.
    pub fn eq(&self, _left: &T, _right: &T) -> Result<bool, HashError> {
        unreachable!("facet-hash native JIT is inactive for this build")
    }

    /// Return zero code-layout counters for the inactive backend.
    #[must_use]
    pub fn stats(&self) -> NativeEqualityPlanStats {
        NativeEqualityPlanStats::default()
    }
}

/// Code-layout counters for a native hash plan.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NativeHashPlanStats {
    /// Number of compiled chains.
    pub chain_count: usize,
    /// Number of stencil copies.
    pub stencil_count: usize,
    /// Number of side program-stream words.
    pub prog_slot_count: usize,
    /// Number of standalone scalar ops.
    pub scalar_count: usize,
    /// Number of grouped scalar runs.
    pub scalar_run_count: usize,
    /// Total scalar fields inside grouped runs.
    pub scalar_run_field_count: usize,
    /// Number of compile-time `usize` constants hashed by this plan.
    pub const_usize_count: usize,
}

/// Code-layout counters for a native equality plan.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NativeEqualityPlanStats {
    /// Number of compiled chains.
    pub chain_count: usize,
    /// Number of stencil copies.
    pub stencil_count: usize,
    /// Number of side program-stream words.
    pub prog_slot_count: usize,
    /// Number of standalone scalar ops.
    pub scalar_count: usize,
    /// Number of grouped scalar runs.
    pub scalar_run_count: usize,
    /// Total scalar fields inside grouped runs.
    pub scalar_run_field_count: usize,
}
