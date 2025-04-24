use core::alloc::Layout;

use crate::{
    ConstTypeId, Def, Facet, PtrConst, RefPtrDef, RefPtrMutability, RefPtrType, Shape, TypeParam,
    ValueVTable,
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
                        Def::Slice(slice) => {
                            let item_shape = slice.t;

                            builder = builder.marker_traits(item_shape.vtable.marker_traits);

                            if item_shape.vtable.debug.is_some() {
                                builder = builder.debug(|value, f| {
                                    let Def::Slice(slice) = T::SHAPE.def else {
                                        unreachable!();
                                    };
                                    let item_shape = slice.t;
                                    let item_layout = item_shape.layout.sized_layout().unwrap();

                                    let len = value.fat_part().unwrap();
                                    let mut ptr = value.as_byte_ptr();

                                    write!(f, "[")?;
                                    for i in 0..len {
                                        if i > 0 {
                                            write!(f, ", ")?;
                                        }
                                        unsafe {
                                            (T::SHAPE.vtable.debug.unwrap_unchecked())(
                                                PtrConst::new(ptr),
                                                f,
                                            )?;
                                        }
                                        ptr = ptr.wrapping_add(item_layout.size());
                                    }
                                    write!(f, "]")
                                });
                            }

                            /*
                            if T::SHAPE.vtable.eq.is_some() {
                                builder = builder.eq(|a, b| {
                                    let a = unsafe { a.get::<&[T]>() };
                                    let b = unsafe { b.get::<&[T]>() };
                                    if a.len() != b.len() {
                                        return false;
                                    }
                                    for (x, y) in a.iter().zip(b.iter()) {
                                        if !unsafe {
                                            (T::SHAPE.vtable.eq.unwrap_unchecked())(
                                                PtrConst::new(x as *const _),
                                                PtrConst::new(y as *const _),
                                            )
                                        } {
                                            return false;
                                        }
                                    }
                                    true
                                });
                            }

                            if T::SHAPE.vtable.ord.is_some() {
                                builder = builder.ord(|a, b| {
                                    let a = unsafe { a.get::<&[T]>() };
                                    let b = unsafe { b.get::<&[T]>() };
                                    for (x, y) in a.iter().zip(b.iter()) {
                                        let ord = unsafe {
                                            (T::SHAPE.vtable.ord.unwrap_unchecked())(
                                                PtrConst::new(x as *const _),
                                                PtrConst::new(y as *const _),
                                            )
                                        };
                                        if ord != core::cmp::Ordering::Equal {
                                            return ord;
                                        }
                                    }
                                    a.len().cmp(&b.len())
                                });
                            }

                            if T::SHAPE.vtable.partial_ord.is_some() {
                                builder = builder.partial_ord(|a, b| {
                                    let a = unsafe { a.get::<&[T]>() };
                                    let b = unsafe { b.get::<&[T]>() };
                                    for (x, y) in a.iter().zip(b.iter()) {
                                        let ord = unsafe {
                                            (T::SHAPE.vtable.partial_ord.unwrap_unchecked())(
                                                PtrConst::new(x as *const _),
                                                PtrConst::new(y as *const _),
                                            )
                                        };
                                        match ord {
                                            Some(core::cmp::Ordering::Equal) => continue,
                                            Some(order) => return Some(order),
                                            None => return None,
                                        }
                                    }
                                    a.len().partial_cmp(&b.len())
                                });
                            }

                            if T::SHAPE.vtable.hash.is_some() {
                                builder = builder.hash(|value, state, hasher| {
                                    let value = unsafe { value.get::<&[T]>() };
                                    for item in value.iter() {
                                        unsafe {
                                            (T::SHAPE.vtable.hash.unwrap_unchecked())(
                                                PtrConst::new(item as *const _),
                                                state,
                                                hasher,
                                            )
                                        };
                                    }
                                });
                            }
                            */

                            builder.build()
                        }
                        _ => {
                            if T::SHAPE.vtable.debug.is_some() {
                                builder = builder.debug(|value, f| {
                                    let v = unsafe { value.get::<Self>() };
                                    let v = *v;
                                    unsafe {
                                        (T::SHAPE.vtable.debug.unwrap_unchecked())(
                                            PtrConst::new(v),
                                            f,
                                        )
                                    }
                                });
                            }

                            // TODO: More functions

                            // TODO: Marker traits
                            // builder = builder.marker_traits(traits);

                            builder.build()
                        }
                    }
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
