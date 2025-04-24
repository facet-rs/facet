use core::alloc::Layout;

use crate::{
    ConstTypeId, Def, Facet, PtrConst, RefPtrDef, RefPtrMutability, RefPtrType, Shape, TypeParam,
    ValueVTable,
};

// TODO: Also implement for `T: !Sized`
// - Conflicts with `&[T]`, `&str` and `&Path`
// TODO: Also implement for `T: !Facet<'a>`
unsafe impl<'a, T: Facet<'a>> Facet<'a> for &'a T {
    const SHAPE: &'static Shape = &const {
        Shape::builder()
            .id(ConstTypeId::of::<Self>())
            .layout(Layout::new::<Self>())
            .type_params(&[TypeParam {
                name: "T",
                shape: || T::SHAPE,
            }])
            .vtable(
                &const {
                    let mut builder = ValueVTable::builder()
                        .type_name(|f, opts| {
                            if let Some(opts) = opts.for_children() {
                                write!(f, "&")?;
                                (T::SHAPE.vtable.type_name)(f, opts)
                            } else {
                                write!(f, "&⋯")
                            }
                        })
                        .drop_in_place(|value| unsafe { value.drop_in_place::<Self>() })
                        .clone_into(|src, dst| unsafe { dst.put(src.get::<Self>()) });

                    if T::SHAPE.vtable.debug.is_some() {
                        builder = builder.debug(|value, f| {
                            let v = unsafe { value.get::<Self>() };
                            let v = *v;
                            unsafe {
                                (T::SHAPE.vtable.debug.unwrap_unchecked())(PtrConst::new(v), f)
                            }
                        });
                    }

                    // TODO: More functions

                    // TODO: Marker traits
                    // builder = builder.marker_traits(traits);

                    builder.build()
                },
            )
            .def(Def::RefPtr(
                RefPtrDef::builder()
                    .typ(RefPtrType::Reference)
                    .mutability(RefPtrMutability::Const)
                    .pointee(T::SHAPE)
                    .build(),
            ))
            .build()
    };
}

unsafe impl<'a, T: Facet<'a>> Facet<'a> for &'a mut T {
    const SHAPE: &'static Shape = &const {
        Shape::builder()
            .id(ConstTypeId::of::<Self>())
            .layout(Layout::new::<Self>())
            .type_params(&[TypeParam {
                name: "T",
                shape: || T::SHAPE,
            }])
            .vtable(
                &const {
                    let mut builder = ValueVTable::builder()
                        .type_name(|f, opts| {
                            if let Some(opts) = opts.for_children() {
                                write!(f, "&mut ")?;
                                (T::SHAPE.vtable.type_name)(f, opts)
                            } else {
                                write!(f, "&mut ⋯")
                            }
                        })
                        .drop_in_place(|value| unsafe { value.drop_in_place::<Self>() })
                        .clone_into(|src, dst| unsafe { dst.put(src.get::<Self>()) });

                    if T::SHAPE.vtable.debug.is_some() {
                        builder = builder.debug(|value, f| {
                            let v = unsafe { value.get::<Self>() } as *const &mut T;
                            let v = unsafe { v.read() };
                            unsafe {
                                (T::SHAPE.vtable.debug.unwrap_unchecked())(PtrConst::new(v), f)
                            }
                        });
                    }

                    // TODO: More functions

                    // TODO: Marker traits
                    // builder = builder.marker_traits(traits);

                    builder.build()
                },
            )
            .def(Def::RefPtr(
                RefPtrDef::builder()
                    .typ(RefPtrType::Reference)
                    .mutability(RefPtrMutability::Mut)
                    .pointee(T::SHAPE)
                    .build(),
            ))
            .build()
    };
}

unsafe impl<'a, T: Facet<'a>> Facet<'a> for *const T {
    const SHAPE: &'static Shape = &const {
        Shape::builder()
            .id(ConstTypeId::of::<Self>())
            .layout(Layout::new::<Self>())
            .type_params(&[TypeParam {
                name: "T",
                shape: || T::SHAPE,
            }])
            .vtable(
                &const {
                    let builder = ValueVTable::builder()
                        .type_name(|f, opts| {
                            if let Some(opts) = opts.for_children() {
                                write!(f, "*const ")?;
                                (T::SHAPE.vtable.type_name)(f, opts)
                            } else {
                                write!(f, "*const ⋯")
                            }
                        })
                        .debug(|value, f| {
                            let v = unsafe { value.get::<Self>() };
                            write!(f, "{v:?}")
                        });

                    // Pointers aren't guaranteed to be valid, so we can't implement most functions

                    // TODO: Marker traits
                    // builder = builder.marker_traits(traits);

                    builder.build()
                },
            )
            .def(Def::RefPtr(
                RefPtrDef::builder()
                    .typ(RefPtrType::Pointer)
                    .mutability(RefPtrMutability::Const)
                    .pointee(T::SHAPE)
                    .build(),
            ))
            .build()
    };
}

unsafe impl<'a, T: Facet<'a>> Facet<'a> for *mut T {
    const SHAPE: &'static Shape = &const {
        Shape::builder()
            .id(ConstTypeId::of::<Self>())
            .layout(Layout::new::<Self>())
            .type_params(&[TypeParam {
                name: "T",
                shape: || T::SHAPE,
            }])
            .vtable(
                &const {
                    let builder = ValueVTable::builder()
                        .type_name(|f, opts| {
                            if let Some(opts) = opts.for_children() {
                                write!(f, "*mut ")?;
                                (T::SHAPE.vtable.type_name)(f, opts)
                            } else {
                                write!(f, "*mut ⋯")
                            }
                        })
                        .debug(|value, f| {
                            let v = unsafe { value.get::<Self>() };
                            write!(f, "{v:?}")
                        });

                    // Pointers aren't guaranteed to be valid, so we can't implement most functions

                    // TODO: Marker traits
                    // builder = builder.marker_traits(traits);

                    builder.build()
                },
            )
            .def(Def::RefPtr(
                RefPtrDef::builder()
                    .typ(RefPtrType::Pointer)
                    .mutability(RefPtrMutability::Mut)
                    .pointee(T::SHAPE)
                    .build(),
            ))
            .build()
    };
}
