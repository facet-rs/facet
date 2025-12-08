use crate::Variance;
use core::fmt;
use core::hash::Hash;

use crate::{
    CmpVTable, Def, Facet, FormatVTable, HashVTable, MarkerTraits, PointerType, Shape, Type,
    TypeParam, ValuePointerType, ValueVTable,
};

// *const pointers
unsafe impl<'a, T: Facet<'a> + ?Sized> Facet<'a> for *const T {
    const SHAPE: &'static Shape = &const {
        Shape {
            id: Shape::id_of::<Self>(),
            layout: Shape::layout_of::<Self>(),
            vtable: ValueVTable {
                type_name: |f, opts| {
                    if let Some(opts) = opts.for_children() {
                        write!(f, "*const ")?;
                        (T::SHAPE.vtable.type_name())(f, opts)
                    } else {
                        write!(f, "*const …")
                    }
                },
                drop_in_place: ValueVTable::drop_in_place_for::<Self>(),
                default_in_place: None,
                clone_into: Some(|src, dst| unsafe { dst.put(*src.get::<Self>()) }),
                parse: None,
                invariants: None,
                try_from: None,
                try_into_inner: None,
                try_borrow_inner: None,
                format: FormatVTable {
                    display: None,
                    debug: Some(|p, f| fmt::Debug::fmt(unsafe { p.get::<Self>() }, f)),
                },
                cmp: CmpVTable {
                    partial_eq: Some(|a, b| {
                        #[allow(ambiguous_wide_pointer_comparisons)]
                        unsafe {
                            *a.get::<Self>() == *b.get::<Self>()
                        }
                    }),
                    partial_ord: Some(|a, b| {
                        #[allow(ambiguous_wide_pointer_comparisons)]
                        unsafe {
                            a.get::<Self>().partial_cmp(b.get::<Self>())
                        }
                    }),
                    ord: Some(|a, b| {
                        #[allow(ambiguous_wide_pointer_comparisons)]
                        unsafe {
                            a.get::<Self>().cmp(b.get::<Self>())
                        }
                    }),
                },
                hash: HashVTable {
                    hash: Some(|value, hasher| unsafe {
                        value.get::<Self>().hash(&mut { hasher })
                    }),
                },
                markers: MarkerTraits::EMPTY.with_eq().with_copy(),
            },
            ty: {
                let is_wide = ::core::mem::size_of::<Self>() != ::core::mem::size_of::<*const ()>();
                let vpt = ValuePointerType {
                    mutable: false,
                    wide: is_wide,
                    target: T::SHAPE,
                };

                Type::Pointer(PointerType::Raw(vpt))
            },
            def: Def::Scalar,
            type_identifier: "*const _",
            type_params: &[TypeParam {
                name: "T",
                shape: T::SHAPE,
            }],
            doc: &[],
            attributes: &[],
            type_tag: None,
            inner: Some(T::SHAPE),
            proxy: None,
            variance: Variance::Invariant,
        }
    };
}

// *mut pointers
unsafe impl<'a, T: Facet<'a> + ?Sized> Facet<'a> for *mut T {
    const SHAPE: &'static Shape = &const {
        Shape {
            id: Shape::id_of::<Self>(),
            layout: Shape::layout_of::<Self>(),
            vtable: ValueVTable {
                type_name: |f, opts| {
                    if let Some(opts) = opts.for_children() {
                        write!(f, "*mut ")?;
                        (T::SHAPE.vtable.type_name())(f, opts)
                    } else {
                        write!(f, "*mut …")
                    }
                },
                drop_in_place: ValueVTable::drop_in_place_for::<Self>(),
                default_in_place: None,
                clone_into: Some(|src, dst| unsafe { dst.put(*src.get::<Self>()) }),
                parse: None,
                invariants: None,
                try_from: None,
                try_into_inner: None,
                try_borrow_inner: None,
                format: FormatVTable {
                    display: None,
                    debug: Some(|p, f| fmt::Debug::fmt(unsafe { p.get::<Self>() }, f)),
                },
                cmp: CmpVTable {
                    partial_eq: Some(|a, b| {
                        #[allow(ambiguous_wide_pointer_comparisons)]
                        unsafe {
                            *a.get::<Self>() == *b.get::<Self>()
                        }
                    }),
                    partial_ord: Some(|a, b| {
                        #[allow(ambiguous_wide_pointer_comparisons)]
                        unsafe {
                            a.get::<Self>().partial_cmp(b.get::<Self>())
                        }
                    }),
                    ord: Some(|a, b| {
                        #[allow(ambiguous_wide_pointer_comparisons)]
                        unsafe {
                            a.get::<Self>().cmp(b.get::<Self>())
                        }
                    }),
                },
                hash: HashVTable {
                    hash: Some(|value, hasher| unsafe {
                        value.get::<Self>().hash(&mut { hasher })
                    }),
                },
                markers: MarkerTraits::EMPTY.with_eq().with_copy(),
            },
            ty: {
                let is_wide = ::core::mem::size_of::<Self>() != ::core::mem::size_of::<*const ()>();
                let vpt = ValuePointerType {
                    mutable: true,
                    wide: is_wide,
                    target: T::SHAPE,
                };

                Type::Pointer(PointerType::Raw(vpt))
            },
            def: Def::Scalar,
            type_identifier: "*mut _",
            type_params: &[TypeParam {
                name: "T",
                shape: T::SHAPE,
            }],
            doc: &[],
            attributes: &[],
            type_tag: None,
            inner: Some(T::SHAPE),
            proxy: None,
            variance: Variance::Invariant,
        }
    };
}

#[cfg(test)]
mod test {
    use core::panic::{RefUnwindSafe, UnwindSafe};
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
    fn mut_ref_not_unwind_safe() {
        assert!(impls!(&mut (): !UnwindSafe));
    }
}
