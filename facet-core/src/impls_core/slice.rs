use crate::*;
use core::alloc::Layout;

unsafe impl<'a, T> Facet<'a> for &'a [T]
where
    T: Facet<'a>,
{
    const SHAPE: &'static Shape = &const {
        Shape::builder()
            .id(ConstTypeId::of::<&[T]>())
            .layout(Layout::new::<&[T]>())
            .type_params(&[
                TypeParam {
                    name: "T",
                    shape: || T::SHAPE,
                }
            ])
            .def(Def::Slice(
                SliceDef::builder()
                    .vtable(
                        &const {
                            SliceVTable::builder()
                                .len(|ptr| unsafe {
                                    let slice = ptr.get::<&[T]>();
                                    slice.len()
                                })
                                .get_item_ptr(|ptr, index| unsafe {
                                    let slice = ptr.get::<&[T]>();
                                    let len = slice.len();
                                    if index >= len {
                                        panic!(
                                            "Index out of bounds: the len is {len} but the index is {index}"
                                        );
                                    }
                                    PtrConst::new(slice.as_ptr().add(index))
                                })
                                .build()
                        },
                    )
                    .t(|| T::SHAPE)
                    .build(),
            ))
            .vtable(
                &const {
                    let mut builder = ValueVTable::builder()
                        .type_name(|f, opts| {
                            if let Some(opts) = opts.for_children() {
                                write!(f, "&[")?;
                                (T::SHAPE.vtable.type_name)(f, opts)?;
                                write!(f, "]")
                            } else {
                                write!(f, "&[⋯]")
                            }
                        })
                        .marker_traits(T::SHAPE.vtable.marker_traits)
                        .default_in_place(|ptr| unsafe { ptr.put(&[] as &[T]) })
                        .clone_into(|src, dst| unsafe {
                            // This works because we're cloning a shared reference (&[T]), not the actual slice data.
                            // We're just copying the fat pointer (ptr + length) that makes up the slice reference.
                            dst.put(src.get::<&[T]>())
                        });

                    if T::SHAPE.vtable.debug.is_some() {
                        builder = builder.debug(|value, f| {
                            let value = unsafe { value.get::<&[T]>() };
                            write!(f, "[")?;
                            for (i, item) in value.iter().enumerate() {
                                if i > 0 {
                                    write!(f, ", ")?;
                                }
                                unsafe {
                                    (T::SHAPE.vtable.debug.unwrap_unchecked())(
                                        PtrConst::new(item as *const _),
                                        f,
                                    )?;
                                }
                            }
                            write!(f, "]")
                        });
                    }

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

                    builder.build()
                },
            )
            .build()
    };
}
