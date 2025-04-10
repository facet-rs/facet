use crate::*;
use core::alloc::Layout;

unsafe impl<T> Facet for [T; 1]
where
    T: Facet,
{
    const SHAPE: &'static Shape = &const {
        Shape::builder()
            .layout(Layout::new::<[T; 1]>())
            .vtable(
                &const {
                    let mut builder = ValueVTable::builder()
                        .marker_traits(T::SHAPE.vtable.marker_traits)
                        .type_name(|f, opts| {
                            if let Some(opts) = opts.for_children() {
                                write!(f, "[")?;
                                (T::SHAPE.vtable.type_name)(f, opts)?;
                                write!(f, "; 1]")
                            } else {
                                write!(f, "[⋯; 1]")
                            }
                        })
                        .drop_in_place(|value| unsafe {
                            core::ptr::drop_in_place(value.as_mut::<[T; 1]>());
                        });
                    if T::SHAPE.vtable.debug.is_some() {
                        builder = builder.debug(|value, f| {
                            let value = unsafe { value.as_ref::<[T; 1]>() };
                            write!(f, "[")?;
                            unsafe {
                                (T::SHAPE.vtable.debug.unwrap_unchecked())(
                                    OpaqueConst::new(&value[0] as *const T),
                                    f,
                                )?;
                            }
                            write!(f, "]")
                        });
                    }
                    if T::SHAPE.vtable.eq.is_some() {
                        builder = builder.eq(|a, b| {
                            let a = unsafe { a.as_ref::<[T; 1]>() };
                            let b = unsafe { b.as_ref::<[T; 1]>() };
                            unsafe {
                                (T::SHAPE.vtable.eq.unwrap_unchecked())(
                                    OpaqueConst::new(&a[0] as *const T),
                                    OpaqueConst::new(&b[0] as *const T),
                                )
                            }
                        });
                    }
                    if T::SHAPE.vtable.default_in_place.is_some() {
                        builder = builder.default_in_place(|target| unsafe {
                            let t_dip = T::SHAPE.vtable.default_in_place.unwrap_unchecked();
                            (t_dip)(target.field_uninit(0))
                        });
                    }
                    if T::SHAPE.vtable.clone_into.is_some() {
                        builder = builder.clone_into(|src, dst| unsafe {
                            let t_cip = T::SHAPE.vtable.clone_into.unwrap_unchecked();
                            (t_cip)(
                                OpaqueConst::new(&src.as_ref::<[T; 1]>()[0] as *const T),
                                dst.field_uninit(0),
                            )
                        });
                    }
                    if T::SHAPE.vtable.partial_ord.is_some() {
                        builder = builder.partial_ord(|a, b| {
                            let a = unsafe { a.as_ref::<[T; 1]>() };
                            let b = unsafe { b.as_ref::<[T; 1]>() };
                            unsafe {
                                (T::SHAPE.vtable.partial_ord.unwrap_unchecked())(
                                    OpaqueConst::new(&a[0] as *const T),
                                    OpaqueConst::new(&b[0] as *const T),
                                )
                            }
                        });
                    }
                    if T::SHAPE.vtable.ord.is_some() {
                        builder = builder.ord(|a, b| {
                            let a = unsafe { a.as_ref::<[T; 1]>() };
                            let b = unsafe { b.as_ref::<[T; 1]>() };
                            unsafe {
                                (T::SHAPE.vtable.ord.unwrap_unchecked())(
                                    OpaqueConst::new(&a[0] as *const T),
                                    OpaqueConst::new(&b[0] as *const T),
                                )
                            }
                        });
                    }
                    if T::SHAPE.vtable.hash.is_some() {
                        builder = builder.hash(|value, state, hasher| {
                            let value = unsafe { value.as_ref::<[T; 1]>() };
                            unsafe {
                                (T::SHAPE.vtable.hash.unwrap_unchecked())(
                                    OpaqueConst::new(&value[0] as *const T),
                                    state,
                                    hasher,
                                )
                            }
                        });
                    }
                    builder.build()
                },
            )
            .def(Def::List(
                ListDef::builder()
                    .vtable(
                        &const {
                            ListVTable::builder()
                        .init_in_place_with_capacity(|_, _| Err(()))
                        .push(|_, _| {
                            panic!("Cannot push to [T; 1]");
                        })
                        .len(|_| 1)
                        .get_item_ptr(|ptr, index| unsafe {
                            if index >= 1 {
                                panic!(
                                    "Index out of bounds: the len is 1 but the index is {index}"
                                );
                            }
                            OpaqueConst::new(ptr.as_ptr::<[T; 1]>())
                        })
                        .build()
                        },
                    )
                    .t(T::SHAPE)
                    .build(),
            ))
            .build()
    };
}
