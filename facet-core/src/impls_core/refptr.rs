use core::alloc::Layout;

use crate::{
    ConstTypeId, Def, Facet, MarkerTraits, PtrConst, RefPtrDef, RefPtrMutability, RefPtrType,
    Shape, TypeNameFn, TypeParam, ValueVTable, ValueVTableBuilder,
};

#[repr(C)]
struct SlicePtr {
    ptr: *const u8,
    len: usize,
}

// Implements traits for `& [U]` where `T = [U]`
const fn slice_ref_impl<'a, T: Facet<'a> + ?Sized>(
    mut builder: ValueVTableBuilder,
    item_shape: &'static Shape,
) -> ValueVTableBuilder {
    builder = builder.default_in_place(|dst| {
        const EMPTY: &[()] = &[];
        unsafe { dst.put(EMPTY) }
    });

    if item_shape.vtable.debug.is_some() {
        builder = builder.debug(|value, f| {
            let Def::Slice(slice) = T::SHAPE.def else {
                unreachable!();
            };
            let item_shape = slice.t;
            let item_layout = item_shape.layout.sized_layout().unwrap();

            let SlicePtr { mut ptr, len } = unsafe { value.as_ptr::<SlicePtr>().read() };

            write!(f, "[")?;
            for i in 0..len {
                if i > 0 {
                    write!(f, ", ")?;
                }
                unsafe {
                    (item_shape.vtable.debug.unwrap_unchecked())(PtrConst::new(ptr), f)?;
                }
                ptr = ptr.wrapping_add(item_layout.size());
            }
            write!(f, "]")
        });
    }

    if item_shape.vtable.eq.is_some() {
        builder = builder.eq(|a, b| {
            let Def::Slice(slice) = T::SHAPE.def else {
                unreachable!();
            };
            let item_shape = slice.t;
            let item_layout = item_shape.layout.sized_layout().unwrap();

            let a = unsafe { a.as_ptr::<SlicePtr>().read() };
            let b = unsafe { b.as_ptr::<SlicePtr>().read() };

            if a.len != b.len {
                return false;
            }

            let mut a_ptr = a.ptr;
            let mut b_ptr = b.ptr;

            for _ in 0..a.len {
                if !unsafe {
                    (item_shape.vtable.eq.unwrap_unchecked())(
                        PtrConst::new(a_ptr),
                        PtrConst::new(b_ptr),
                    )
                } {
                    return false;
                }
                a_ptr = a_ptr.wrapping_add(item_layout.size());
                b_ptr = b_ptr.wrapping_add(item_layout.size());
            }
            true
        });
    }

    if item_shape.vtable.ord.is_some() {
        builder = builder.ord(|a, b| {
            let Def::Slice(slice) = T::SHAPE.def else {
                unreachable!();
            };
            let item_shape = slice.t;
            let item_layout = item_shape.layout.sized_layout().unwrap();

            let a = unsafe { a.as_ptr::<SlicePtr>().read() };
            let b = unsafe { b.as_ptr::<SlicePtr>().read() };

            let min = a.len.min(b.len);
            let mut a_ptr = a.ptr;
            let mut b_ptr = b.ptr;

            for _ in 0..min {
                let ord = unsafe {
                    (item_shape.vtable.ord.unwrap_unchecked())(
                        PtrConst::new(a_ptr),
                        PtrConst::new(b_ptr),
                    )
                };
                if ord != core::cmp::Ordering::Equal {
                    return ord;
                }
                a_ptr = a_ptr.wrapping_add(item_layout.size());
                b_ptr = b_ptr.wrapping_add(item_layout.size());
            }

            a.len.cmp(&b.len)
        });
    }

    if item_shape.vtable.partial_ord.is_some() {
        builder = builder.partial_ord(|a, b| {
            let Def::Slice(slice) = T::SHAPE.def else {
                unreachable!();
            };
            let item_shape = slice.t;
            let item_layout = item_shape.layout.sized_layout().unwrap();

            let a = unsafe { a.as_ptr::<SlicePtr>().read() };
            let b = unsafe { b.as_ptr::<SlicePtr>().read() };

            let min = a.len.min(b.len);
            let mut a_ptr = a.ptr;
            let mut b_ptr = b.ptr;

            for _ in 0..min {
                let ord = unsafe {
                    (item_shape.vtable.partial_ord.unwrap_unchecked())(
                        PtrConst::new(a_ptr),
                        PtrConst::new(b_ptr),
                    )
                };
                match ord {
                    Some(core::cmp::Ordering::Equal) => {}
                    Some(order) => return Some(order),
                    None => return None,
                }
                a_ptr = a_ptr.wrapping_add(item_layout.size());
                b_ptr = b_ptr.wrapping_add(item_layout.size());
            }

            a.len.partial_cmp(&b.len)
        });
    }

    if item_shape.vtable.hash.is_some() {
        builder = builder.hash(|value, state, hasher| {
            let Def::Slice(slice) = T::SHAPE.def else {
                unreachable!();
            };
            let item_shape = slice.t;
            let item_layout = item_shape.layout.sized_layout().unwrap();

            let SlicePtr { mut ptr, len } = unsafe { value.as_ptr::<SlicePtr>().read() };

            for _ in 0..len {
                unsafe {
                    (item_shape.vtable.hash.unwrap_unchecked())(PtrConst::new(ptr), state, hasher)
                };
                ptr = ptr.wrapping_add(item_layout.size());
            }
        });
    }

    builder
}

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
                    let builder = ValueVTable::builder()
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

                    let builder = match T::SHAPE.def {
                        // Slice reference &[U]
                        Def::Slice(slice) => {
                            let item_shape = slice.t;
                            slice_ref_impl::<T>(builder, item_shape)
                        }
                        Def::Str => {
                            let builder = slice_ref_impl::<T>(builder, u8::SHAPE);
                            builder.debug(|_value, f| write!(f, "&str TODO"))
                        }
                        _ => {
                            /*
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
                            */

                            // TODO: More functions

                            // TODO: Marker traits
                            // builder = builder.marker_traits(traits);

                            builder
                        }
                    };

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
