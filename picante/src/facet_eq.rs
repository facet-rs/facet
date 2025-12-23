//! Lightweight equality checking for Facet types without pulling in facet-diff.
//!
//! This module provides equality checking by using facet's built-in vtable for PartialEq.
//! When a type implements PartialEq, facet's reflection system provides a vtable entry
//! that we can call to perform the comparison. For types without PartialEq, we return false
//! (conservative fallback).

use facet::Facet;
use std::any::Any;

/// Check if two type-erased Facet values are equal.
///
/// This function:
/// - Downcasts both `dyn Any` values to the concrete type `V`
/// - Uses facet's vtable `partial_eq` function if available
/// - Returns false if the type doesn't implement PartialEq
///
/// # Type Parameters
/// - `V`: Must implement `Facet<'static>` for shape information
///
/// # Returns
/// - `true` if values are equal
/// - `false` if types don't match, values differ, or type doesn't implement PartialEq
pub fn facet_eq<V>(a: &dyn Any, b: &dyn Any) -> bool
where
    V: Facet<'static> + 'static,
{
    // Downcast to concrete type
    let Some(a) = a.downcast_ref::<V>() else {
        return false;
    };
    let Some(b) = b.downcast_ref::<V>() else {
        return false;
    };

    facet_eq_direct(a, b)
}

/// Check if two concrete Facet values are equal.
///
/// Uses facet's reflection vtable to call the PartialEq implementation if it exists.
/// This is the proper way to do equality comparison with facet - using the vtable
/// entry that facet generates for types implementing PartialEq.
///
/// # Type Parameters
/// - `V`: Must implement `Facet<'static>` for shape information
///
/// # Returns
/// - `true` if values are equal
/// - `false` if values differ or type doesn't implement PartialEq
#[inline]
pub fn facet_eq_direct<V>(a: &V, b: &V) -> bool
where
    V: Facet<'static> + 'static,
{
    let shape = V::SHAPE;

    // Use facet's vtable to call partial_eq if available
    // SAFETY: We're passing valid pointers to values of type V, which matches the shape
    unsafe {
        shape.call_partial_eq(
            facet::PtrConst::new(a as *const V as *const ()),
            facet::PtrConst::new(b as *const V as *const ()),
        )
    }
    .unwrap_or(false) // If partial_eq is None, the type doesn't implement PartialEq
}
