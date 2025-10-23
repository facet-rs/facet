use core::fmt;
use core::hash::Hash;

use crate::{
    Facet, MarkerTraits, PointerType, Shape, Type, TypeParam, ValuePointerType, ValueVTable,
};

// *const pointers
unsafe impl<'a, T: Facet<'a> + ?Sized> Facet<'a> for *const T {
    const SHAPE: &'static Shape = &const {
        Shape::builder_for_sized::<Self>()
            .vtable(
                ValueVTable::builder::<Self>()
                    .marker_traits({
                        let mut marker_traits = MarkerTraits::EQ
                            .union(MarkerTraits::COPY)
                            .union(MarkerTraits::UNPIN);

                        if T::SHAPE
                            .vtable
                            .marker_traits()
                            .contains(MarkerTraits::REF_UNWIND_SAFE)
                        {
                            marker_traits = marker_traits
                                .union(MarkerTraits::UNWIND_SAFE)
                                .union(MarkerTraits::REF_UNWIND_SAFE);
                        }

                        marker_traits
                    })
                    .debug(Some(|p, f| fmt::Debug::fmt(p.get(), f)))
                    .partial_eq(Some(|a, b| {
                        #[allow(ambiguous_wide_pointer_comparisons)]
                        (*a.get() == *b.get())
                    }))
                    .partial_ord(Some(|a, b| {
                        #[allow(ambiguous_wide_pointer_comparisons)]
                        (a.get().partial_cmp(b.get()))
                    }))
                    .ord(Some(|a, b| {
                        #[allow(ambiguous_wide_pointer_comparisons)]
                        (a.get().cmp(b.get()))
                    }))
                    .hash(Some(|value, hasher| value.get().hash(&mut { hasher })))
                    .clone_into(Some(|src, dst| unsafe { dst.put(*src.get()).into() }))
                    .type_name(|f, opts| {
                        if let Some(opts) = opts.for_children() {
                            write!(f, "*const ")?;
                            (T::SHAPE.vtable.type_name())(f, opts)
                        } else {
                            write!(f, "*const …")
                        }
                    })
                    .build(),
            )
            .inner(T::SHAPE)
            .type_identifier("*const _")
            .type_params(&[TypeParam {
                name: "T",
                shape: T::SHAPE,
            }])
            .ty({
                let is_wide = ::core::mem::size_of::<Self>() != ::core::mem::size_of::<*const ()>();
                let vpt = ValuePointerType {
                    mutable: false,
                    wide: is_wide,
                    target: T::SHAPE,
                };

                Type::Pointer(PointerType::Raw(vpt))
            })
            .build()
    };
}

// *mut pointers
unsafe impl<'a, T: Facet<'a> + ?Sized> Facet<'a> for *mut T {
    const SHAPE: &'static Shape = &const {
        Shape::builder_for_sized::<Self>()
            .vtable(
                ValueVTable::builder::<Self>()
                    .marker_traits({
                        let mut marker_traits = MarkerTraits::EQ
                            .union(MarkerTraits::COPY)
                            .union(MarkerTraits::UNPIN);

                        if T::SHAPE
                            .vtable
                            .marker_traits()
                            .contains(MarkerTraits::REF_UNWIND_SAFE)
                        {
                            marker_traits = marker_traits
                                .union(MarkerTraits::UNWIND_SAFE)
                                .union(MarkerTraits::REF_UNWIND_SAFE);
                        }

                        marker_traits
                    })
                    .partial_eq(Some(|a, b| {
                        #[allow(ambiguous_wide_pointer_comparisons)]
                        (*a.get() == *b.get())
                    }))
                    .partial_ord(Some(|a, b| {
                        #[allow(ambiguous_wide_pointer_comparisons)]
                        (a.get().partial_cmp(b.get()))
                    }))
                    .ord(Some(|a, b| {
                        #[allow(ambiguous_wide_pointer_comparisons)]
                        (a.get().cmp(b.get()))
                    }))
                    .hash(Some(|value, hasher| value.get().hash(&mut { hasher })))
                    .debug(Some(|p, f| fmt::Debug::fmt(p.get(), f)))
                    .clone_into(Some(|src, dst| unsafe { dst.put(*src.get()).into() }))
                    .type_name(|f, opts| {
                        if let Some(opts) = opts.for_children() {
                            write!(f, "*mut ")?;
                            (T::SHAPE.vtable.type_name())(f, opts)
                        } else {
                            write!(f, "*mut …")
                        }
                    })
                    .build(),
            )
            .inner(T::SHAPE)
            .type_identifier("*mut _")
            .type_params(&[TypeParam {
                name: "T",
                shape: T::SHAPE,
            }])
            .ty({
                let is_wide = ::core::mem::size_of::<Self>() != ::core::mem::size_of::<*const ()>();
                let vpt = ValuePointerType {
                    mutable: true,
                    wide: is_wide,
                    target: T::SHAPE,
                };

                Type::Pointer(PointerType::Raw(vpt))
            })
            .build()
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
