use core::alloc::Layout;

use crate::{
    ConstTypeId, Def, Facet, MarkerTraits, PtrConst, RefPtrDef, RefPtrMutability, RefPtrType,
    Shape, TypeNameFn, TypeParam, ValueVTable,
};

// TODO: Also implement for `T: !Facet<'a>`
unsafe impl<'a, T: Facet<'a> + ?Sized> Facet<'a> for &'a T {
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

                    match T::SHAPE.def {
                        // Slice reference &[U]
                        Def::Slice(_) => {
                            builder = builder.default_in_place(|dst| {
                                let Def::Slice(slice) = T::SHAPE.def else {
                                    unreachable!();
                                };
                                let item_layout = slice.t.layout.sized_layout().unwrap();

                                let data: *const () =
                                    core::ptr::without_provenance(item_layout.align());
                                let slice = unsafe { core::slice::from_raw_parts(data, 0) };
                                unsafe { dst.put(slice) }
                            });
                        }
                        Def::Str => {
                            builder = builder.default_in_place(|dst| unsafe { dst.put("") });
                        }
                        _ => {}
                    }

                    if T::SHAPE.vtable.display.is_some() {
                        builder = builder.display(|value, f| {
                            let value = unsafe { *value.get::<Self>() };
                            unsafe {
                                (T::SHAPE.vtable.display.unwrap_unchecked())(
                                    PtrConst::new(value),
                                    f,
                                )
                            }
                        });
                    }

                    if T::SHAPE.vtable.debug.is_some() {
                        builder = builder.debug(|value, f| {
                            let value = unsafe { *value.get::<Self>() };
                            unsafe {
                                (T::SHAPE.vtable.debug.unwrap_unchecked())(PtrConst::new(value), f)
                            }
                        });
                    }

                    if T::SHAPE.vtable.eq.is_some() {
                        builder = builder.eq(|a, b| {
                            let a = unsafe { *a.get::<Self>() };
                            let b = unsafe { *b.get::<Self>() };
                            unsafe {
                                (T::SHAPE.vtable.eq.unwrap_unchecked())(
                                    PtrConst::new(a),
                                    PtrConst::new(b),
                                )
                            }
                        });
                    }

                    if T::SHAPE.vtable.ord.is_some() {
                        builder = builder.ord(|a, b| {
                            let a = unsafe { *a.get::<Self>() };
                            let b = unsafe { *b.get::<Self>() };
                            unsafe {
                                (T::SHAPE.vtable.ord.unwrap_unchecked())(
                                    PtrConst::new(a),
                                    PtrConst::new(b),
                                )
                            }
                        });
                    }

                    if T::SHAPE.vtable.partial_ord.is_some() {
                        builder = builder.partial_ord(|a, b| {
                            let a = unsafe { *a.get::<Self>() };
                            let b = unsafe { *b.get::<Self>() };
                            unsafe {
                                (T::SHAPE.vtable.partial_ord.unwrap_unchecked())(
                                    PtrConst::new(a),
                                    PtrConst::new(b),
                                )
                            }
                        });
                    }

                    if T::SHAPE.vtable.hash.is_some() {
                        builder = builder.hash(|value, state, hasher| {
                            let value = unsafe { *value.get::<Self>() };
                            unsafe {
                                (T::SHAPE.vtable.hash.unwrap_unchecked())(
                                    PtrConst::new(value),
                                    state,
                                    hasher,
                                )
                            }
                        });
                    }

                    let t_marker_traits = T::SHAPE.vtable.marker_traits;

                    let mut marker_traits = MarkerTraits::COPY.union(MarkerTraits::UNPIN);

                    if t_marker_traits.contains(MarkerTraits::EQ) {
                        marker_traits = marker_traits.union(MarkerTraits::EQ);
                    }
                    if t_marker_traits.contains(MarkerTraits::SYNC) {
                        marker_traits = marker_traits
                            .union(MarkerTraits::SEND)
                            .union(MarkerTraits::SYNC);
                    }

                    builder.marker_traits(marker_traits).build()
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

unsafe impl<'a, T: Facet<'a> + ?Sized> Facet<'a> for &'a mut T {
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
                    let ref_vtable = <&T as Facet>::SHAPE.vtable;
                    let type_name: TypeNameFn = |f, opts| {
                        if let Some(opts) = opts.for_children() {
                            write!(f, "&mut ")?;
                            (T::SHAPE.vtable.type_name)(f, opts)
                        } else {
                            write!(f, "&mut ⋯")
                        }
                    };

                    let t_marker_traits = T::SHAPE.vtable.marker_traits;

                    let mut marker_traits = MarkerTraits::UNPIN;

                    if t_marker_traits.contains(MarkerTraits::EQ) {
                        marker_traits = marker_traits.union(MarkerTraits::EQ);
                    }
                    if t_marker_traits.contains(MarkerTraits::SEND) {
                        marker_traits = marker_traits.union(MarkerTraits::SEND);
                    }
                    if t_marker_traits.contains(MarkerTraits::SYNC) {
                        marker_traits = marker_traits.union(MarkerTraits::SYNC);
                    }

                    ValueVTable {
                        type_name,
                        marker_traits,
                        ..*ref_vtable
                    }
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

unsafe impl<'a, T: Facet<'a> + ?Sized> Facet<'a> for *const T {
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

unsafe impl<'a, T: Facet<'a> + ?Sized> Facet<'a> for *mut T {
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
