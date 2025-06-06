//! Dynamic trait object support for `Facet` types.
//!
//! This module provides the `DynFacet` trait, which enables dynamic dispatch
//! for types implementing `Facet`. This allows working with heterogeneous
//! collections of `Facet` types through trait objects.
//!
//! # Example
//!
//! ```ignore
//! let s = String::from("hello");
//! let n = 42;
//! let values: Vec<&dyn DynFacet> = vec![&s as &dyn DynFacet, &n as &dyn DynFacet];
//! ```

use crate::{DebugFn, Facet, PtrConst, Shape, Type, UserType, ValueVTable};

/// A curried debug function that captures a pointer to a value and its debug function.
///
/// This struct allows deferring the execution of a debug function while maintaining
/// the necessary context (pointer and function) to execute it later.
pub(crate) struct DebugFnCurried<'mem> {
    ptr: PtrConst<'mem>,
    f: DebugFn,
}

impl<'mem> DebugFnCurried<'mem> {
    /// Calls the stored debug function with the captured pointer.
    ///
    /// # Safety
    ///
    /// The caller must ensure that:
    /// - The pointer is still valid and points to the expected type
    /// - The debug function is compatible with the pointed-to value
    unsafe fn call(self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        unsafe { (self.f)(self.ptr, f) }
    }
}

/// A trait for types that can be used as dynamic `Facet` trait objects.
///
/// This trait provides dynamic dispatch capabilities for `Facet` types,
/// allowing them to be used in heterogeneous collections and other scenarios
/// where the concrete type is not known at compile time.
///
/// # Safety
///
/// This trait is unsafe to implement manually. The provided blanket implementation
/// for all `T: Facet<'a>` should be sufficient for all use cases.
pub unsafe trait DynFacet<'a> {
    /// Returns a curried debug function for this value, if available.
    ///
    /// This allows dynamic dispatch of the debug formatting functionality.
    fn debug(&self) -> Option<DebugFnCurried>;
}

// Blanket implementation of `DynFacet` for all types implementing `Facet`
unsafe impl<'a, T: Facet<'a>> DynFacet<'a> for T {
    fn debug(&self) -> Option<DebugFnCurried> {
        let debug = (T::VTABLE.sized().unwrap().debug)()?;
        Some(DebugFnCurried {
            ptr: PtrConst::new(self),
            f: debug,
        })
    }
}

// Implementation of `Facet` for the `dyn DynFacet` trait object itself,
// enabling nested dynamic dispatch and allowing trait objects to be used
// wherever a `Facet` is expected.
unsafe impl<'a> Facet<'a> for dyn DynFacet<'a> + 'a {
    const VTABLE: &'static ValueVTable = &const {
        ValueVTable::builder_unsized::<Self>()
            .type_name(|f, _opts| write!(f, "dyn DynFacet"))
            .debug(|| {
                Some(|v, f| {
                    if let Some(debug) = v.debug() {
                        unsafe { debug.call(f) }
                    } else {
                        write!(f, "<No Debug impl>")
                    }
                })
            })
            .build()
    };

    const SHAPE: &'static Shape<'static> = &Shape::builder_for_unsized::<Self>()
        .ty(Type::User(UserType::Opaque))
        .type_identifier("dyn DynFacet")
        .build();
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{DebugFnTyped, VTableView};

    /// Helper function to get the debug string representation of a `Facet` value.
    fn debug_str<'a, T: Facet<'a> + ?Sized>(v: &T) -> Option<String> {
        let view = VTableView::<T>::of();
        let debug = view.debug()?;

        struct Debugger<'a, T: ?Sized>(&'a T, DebugFnTyped<T>);
        impl<'a, T: ?Sized> core::fmt::Debug for Debugger<'a, T> {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                (self.1)(self.0, f)
            }
        }

        Some(format!("{:?}", Debugger(v, debug)))
    }

    /// Tests the functionality of `dyn DynFacet` trait objects.
    ///
    /// This test demonstrates:
    /// - Creating trait objects from different concrete types
    /// - Using trait objects in collections (slices and arrays)
    /// - Debug formatting of trait objects
    /// - Nested trait objects (trait object containing other trait objects)
    #[test]
    fn test_dyn() {
        let s = String::from("abc");
        let s_dyn = &s as &dyn DynFacet;

        assert_eq!(debug_str(s_dyn).as_deref(), Some(r#""abc""#),);

        let vec = vec![1, 2, 3];

        let slice: &[&dyn DynFacet] = &[s_dyn, &vec as &dyn DynFacet, &10 as &dyn DynFacet];
        assert_eq!(
            debug_str(slice).as_deref(),
            Some(r#"["abc", [1, 2, 3], 10]"#),
        );

        let arr: [&dyn DynFacet; 3] = [s_dyn, &vec as &dyn DynFacet, &10 as &dyn DynFacet];
        let arr_dyn: &dyn DynFacet = &arr as &dyn DynFacet;
        assert_eq!(
            debug_str(arr_dyn).as_deref(),
            Some(r#"["abc", [1, 2, 3], 10]"#),
        );
    }
}
