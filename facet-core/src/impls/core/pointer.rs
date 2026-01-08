//! Facet implementation for raw pointers (*const T, *mut T)

use core::cmp::Ordering;
use core::hash::Hash;

use crate::{
    Def, Facet, HashProxy, OxPtrConst, OxPtrMut, PointerType, Shape, ShapeBuilder, Type,
    TypeOpsIndirect, TypeParam, VTableIndirect, ValuePointerType, VarianceDesc,
};

// For raw pointers, we use indirect vtable since they're generic over T
// However, they're scalars so the implementations are simple

/// Debug for *const T
unsafe fn const_ptr_debug<T: ?Sized>(
    ox: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    let p = unsafe { ox.ptr().get::<*const T>() };
    Some(core::fmt::Debug::fmt(&p, f))
}

/// Hash for *const T
unsafe fn const_ptr_hash<T: ?Sized>(ox: OxPtrConst, hasher: &mut HashProxy<'_>) -> Option<()> {
    let p = unsafe { ox.ptr().get::<*const T>() };
    p.hash(hasher);
    Some(())
}

/// PartialEq for *const T
#[allow(ambiguous_wide_pointer_comparisons)]
unsafe fn const_ptr_partial_eq<T: ?Sized>(a: OxPtrConst, b: OxPtrConst) -> Option<bool> {
    let a_val = unsafe { a.ptr().get::<*const T>() };
    let b_val = unsafe { b.ptr().get::<*const T>() };
    Some(*a_val == *b_val)
}

/// PartialOrd for *const T
#[allow(ambiguous_wide_pointer_comparisons)]
unsafe fn const_ptr_partial_cmp<T: ?Sized>(
    a: OxPtrConst,
    b: OxPtrConst,
) -> Option<Option<Ordering>> {
    let a_val = unsafe { a.ptr().get::<*const T>() };
    let b_val = unsafe { b.ptr().get::<*const T>() };
    Some(a_val.partial_cmp(b_val))
}

/// Ord for *const T
#[allow(ambiguous_wide_pointer_comparisons)]
unsafe fn const_ptr_cmp<T: ?Sized>(a: OxPtrConst, b: OxPtrConst) -> Option<Ordering> {
    let a_val = unsafe { a.ptr().get::<*const T>() };
    let b_val = unsafe { b.ptr().get::<*const T>() };
    Some(a_val.cmp(b_val))
}

/// Drop for *const T (no-op, pointers don't need dropping)
unsafe fn const_ptr_drop<T: ?Sized>(_ptr: OxPtrMut) {
    // Pointers don't need dropping
}

/// Clone for *const T (Copy types can be cloned by copying)
unsafe fn const_ptr_clone<T: ?Sized>(src: OxPtrConst, dst: OxPtrMut) {
    let src_val = unsafe { src.ptr().get::<*const T>() };
    let dst_ptr = unsafe { dst.ptr().as_mut::<*const T>() };
    *dst_ptr = *src_val;
}

/// Debug for *mut T
unsafe fn mut_ptr_debug<T: ?Sized>(
    ox: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    let p = unsafe { ox.ptr().get::<*mut T>() };
    Some(core::fmt::Debug::fmt(&p, f))
}

/// Hash for *mut T
unsafe fn mut_ptr_hash<T: ?Sized>(ox: OxPtrConst, hasher: &mut HashProxy<'_>) -> Option<()> {
    let p = unsafe { ox.ptr().get::<*mut T>() };
    p.hash(hasher);
    Some(())
}

/// PartialEq for *mut T
#[allow(ambiguous_wide_pointer_comparisons)]
unsafe fn mut_ptr_partial_eq<T: ?Sized>(a: OxPtrConst, b: OxPtrConst) -> Option<bool> {
    let a_val = unsafe { a.ptr().get::<*mut T>() };
    let b_val = unsafe { b.ptr().get::<*mut T>() };
    Some(*a_val == *b_val)
}

/// PartialOrd for *mut T
#[allow(ambiguous_wide_pointer_comparisons)]
unsafe fn mut_ptr_partial_cmp<T: ?Sized>(a: OxPtrConst, b: OxPtrConst) -> Option<Option<Ordering>> {
    let a_val = unsafe { a.ptr().get::<*mut T>() };
    let b_val = unsafe { b.ptr().get::<*mut T>() };
    Some(a_val.partial_cmp(b_val))
}

/// Ord for *mut T
#[allow(ambiguous_wide_pointer_comparisons)]
unsafe fn mut_ptr_cmp<T: ?Sized>(a: OxPtrConst, b: OxPtrConst) -> Option<Ordering> {
    let a_val = unsafe { a.ptr().get::<*mut T>() };
    let b_val = unsafe { b.ptr().get::<*mut T>() };
    Some(a_val.cmp(b_val))
}

/// Drop for *mut T (no-op, pointers don't need dropping)
unsafe fn mut_ptr_drop<T: ?Sized>(_ptr: OxPtrMut) {
    // Pointers don't need dropping
}

/// Clone for *mut T (Copy types can be cloned by copying)
unsafe fn mut_ptr_clone<T: ?Sized>(src: OxPtrConst, dst: OxPtrMut) {
    let src_val = unsafe { src.ptr().get::<*mut T>() };
    let dst_ptr = unsafe { dst.ptr().as_mut::<*mut T>() };
    *dst_ptr = *src_val;
}

// *const pointers
unsafe impl<'a, T: Facet<'a> + ?Sized> Facet<'a> for *const T {
    const SHAPE: &'static Shape = &const {
        const fn build_const_ptr_vtable<'a, T: Facet<'a> + ?Sized>() -> VTableIndirect {
            VTableIndirect {
                display: None,
                debug: Some(const_ptr_debug::<T>),
                hash: Some(const_ptr_hash::<T>),
                invariants: None,
                parse: None,
                parse_bytes: None,
                try_from: None,
                try_into_inner: None,
                try_borrow_inner: None,
                partial_eq: Some(const_ptr_partial_eq::<T>),
                partial_cmp: Some(const_ptr_partial_cmp::<T>),
                cmp: Some(const_ptr_cmp::<T>),
            }
        }

        const fn build_const_ptr_type_ops<T: ?Sized>() -> TypeOpsIndirect {
            TypeOpsIndirect {
                drop_in_place: const_ptr_drop::<T>,
                default_in_place: None,
                clone_into: Some(const_ptr_clone::<T>),
                is_truthy: None,
            }
        }

        ShapeBuilder::for_sized::<*const T>("*const T")
            .decl_id(crate::DeclId::new(crate::decl_id_hash("*const T")))
            .ty({
                let is_wide = ::core::mem::size_of::<Self>() != ::core::mem::size_of::<*const ()>();
                let vpt = ValuePointerType {
                    mutable: false,
                    wide: is_wide,
                    target: T::SHAPE,
                };
                Type::Pointer(PointerType::Raw(vpt))
            })
            .def(Def::Scalar)
            .type_params(&[TypeParam {
                name: "T",
                shape: T::SHAPE,
            }])
            .inner(T::SHAPE)
            .vtable_indirect(&const { build_const_ptr_vtable::<T>() })
            .type_ops_indirect(&const { build_const_ptr_type_ops::<T>() })
            .eq()
            .copy()
            .build()
    };
}

// *mut pointers
unsafe impl<'a, T: Facet<'a> + ?Sized> Facet<'a> for *mut T {
    const SHAPE: &'static Shape = &const {
        const fn build_mut_ptr_vtable<'a, T: Facet<'a> + ?Sized>() -> VTableIndirect {
            VTableIndirect {
                display: None,
                debug: Some(mut_ptr_debug::<T>),
                hash: Some(mut_ptr_hash::<T>),
                invariants: None,
                parse: None,
                parse_bytes: None,
                try_from: None,
                try_into_inner: None,
                try_borrow_inner: None,
                partial_eq: Some(mut_ptr_partial_eq::<T>),
                partial_cmp: Some(mut_ptr_partial_cmp::<T>),
                cmp: Some(mut_ptr_cmp::<T>),
            }
        }

        const fn build_mut_ptr_type_ops<T: ?Sized>() -> TypeOpsIndirect {
            TypeOpsIndirect {
                drop_in_place: mut_ptr_drop::<T>,
                default_in_place: None,
                clone_into: Some(mut_ptr_clone::<T>),
                is_truthy: None,
            }
        }

        ShapeBuilder::for_sized::<*mut T>("*mut T")
            .decl_id(crate::DeclId::new(crate::decl_id_hash("*mut T")))
            .ty({
                let is_wide = ::core::mem::size_of::<Self>() != ::core::mem::size_of::<*const ()>();
                let vpt = ValuePointerType {
                    mutable: true,
                    wide: is_wide,
                    target: T::SHAPE,
                };
                Type::Pointer(PointerType::Raw(vpt))
            })
            .def(Def::Scalar)
            .type_params(&[TypeParam {
                name: "T",
                shape: T::SHAPE,
            }])
            .inner(T::SHAPE)
            .vtable_indirect(&const { build_mut_ptr_vtable::<T>() })
            .type_ops_indirect(&const { build_mut_ptr_type_ops::<T>() })
            // *mut T is invariant with respect to T (per Rust Reference)
            .variance(VarianceDesc::INVARIANT)
            .eq()
            .copy()
            .build()
    };
}

#[cfg(test)]
mod test {
    use core::panic::{RefUnwindSafe, UnwindSafe};

    #[cfg(feature = "auto-traits")]
    use impls::impls;

    #[allow(unused)]
    const fn assert_impls_unwind_safe<T: UnwindSafe>() {}
    #[allow(unused)]
    const fn assert_impls_ref_unwind_safe<T: RefUnwindSafe>() {}

    #[allow(unused)]
    const fn ref_unwind_safe<T: RefUnwindSafe>() {
        assert_impls_unwind_safe::<&T>();
        assert_impls_ref_unwind_safe::<&T>();

        assert_impls_ref_unwind_safe::<&mut T>();

        assert_impls_unwind_safe::<*const T>();
        assert_impls_ref_unwind_safe::<*const T>();

        assert_impls_unwind_safe::<*mut T>();
        assert_impls_ref_unwind_safe::<*mut T>();
    }

    #[test]
    #[cfg(feature = "auto-traits")]
    fn mut_ref_not_unwind_safe() {
        assert!(impls!(&mut (): !UnwindSafe));
    }
}
