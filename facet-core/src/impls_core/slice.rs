use crate::*;

#[repr(C)]
struct SlicePtr<T> {
    ptr: *const T,
    len: usize,
}

impl<T> Clone for SlicePtr<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for SlicePtr<T> {}

impl<T> SlicePtr<T> {
    fn new(value: PtrConst<'_>) -> Self {
        let len = unsafe { value.fat_part().unwrap_unchecked() };
        let ptr = unsafe { value.as_ptr::<T>() };
        Self { ptr, len }
    }
}

struct SlicePtrIterator<T> {
    ptr: *const T,
    remaining: usize,
}

impl<T> Iterator for SlicePtrIterator<T> {
    type Item = *const T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            None
        } else {
            self.remaining -= 1;
            let res = self.ptr;
            self.ptr = self.ptr.wrapping_add(1);
            Some(res)
        }
    }
}

impl<T> IntoIterator for SlicePtr<T> {
    type Item = *const T;

    type IntoIter = SlicePtrIterator<T>;

    fn into_iter(self) -> Self::IntoIter {
        SlicePtrIterator {
            ptr: self.ptr,
            remaining: self.len,
        }
    }
}

unsafe impl<'a, T> Facet<'a> for [T]
where
    T: Facet<'a>,
{
    const SHAPE: &'static Shape = &const {
        Shape::builder()
            .id(ConstTypeId::of::<Self>())
            .set_unsized()
            .type_params(&[TypeParam {
                name: "T",
                shape: || T::SHAPE,
            }])
            .def(Def::Slice(
                SliceDef::builder()
                    .vtable(
                        &const {
                            SliceVTable::builder()
                                .get_item_ptr(|ptr, index| unsafe {
                                    let ptr = ptr.as_ptr::<T>();
                                    PtrConst::new(ptr.wrapping_add(index))
                                })
                                .build()
                        },
                    )
                    .t(T::SHAPE)
                    .build(),
            ))
            .vtable(
                &const {
                    let mut builder = ValueVTable::builder()
                        .type_name(|f, opts| {
                            if let Some(opts) = opts.for_children() {
                                write!(f, "[")?;
                                (T::SHAPE.vtable.type_name)(f, opts)?;
                                write!(f, "]")
                            } else {
                                write!(f, "[⋯]")
                            }
                        })
                        .marker_traits(T::SHAPE.vtable.marker_traits);

                    if T::SHAPE.vtable.debug.is_some() {
                        builder = builder.debug(|value, f| {
                            let ptr = SlicePtr::<T>::new(value);

                            write!(f, "[")?;
                            for (i, item) in ptr.into_iter().enumerate() {
                                if i > 0 {
                                    write!(f, ", ")?;
                                }
                                unsafe {
                                    (T::SHAPE.vtable.debug.unwrap_unchecked())(
                                        PtrConst::new(item),
                                        f,
                                    )?;
                                }
                            }
                            write!(f, "]")
                        });

                        if T::SHAPE.vtable.eq.is_some() {
                            builder = builder.eq(|a, b| {
                                let a = SlicePtr::<T>::new(a);
                                let b = SlicePtr::<T>::new(b);

                                if a.len != b.len {
                                    return false;
                                }

                                for (a, b) in a.into_iter().zip(b) {
                                    if !unsafe {
                                        (T::SHAPE.vtable.eq.unwrap_unchecked())(
                                            PtrConst::new(a),
                                            PtrConst::new(b),
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
                                let a = SlicePtr::<T>::new(a);
                                let b = SlicePtr::<T>::new(b);

                                for (a, b) in a.into_iter().zip(b) {
                                    let ord = unsafe {
                                        (T::SHAPE.vtable.ord.unwrap_unchecked())(
                                            PtrConst::new(a),
                                            PtrConst::new(b),
                                        )
                                    };
                                    if ord != core::cmp::Ordering::Equal {
                                        return ord;
                                    }
                                }

                                a.len.cmp(&b.len)
                            });
                        }

                        if T::SHAPE.vtable.partial_ord.is_some() {
                            builder = builder.partial_ord(|a, b| {
                                let a = SlicePtr::<T>::new(a);
                                let b = SlicePtr::<T>::new(b);

                                for (a, b) in a.into_iter().zip(b) {
                                    let ord = unsafe {
                                        (T::SHAPE.vtable.partial_ord.unwrap_unchecked())(
                                            PtrConst::new(a),
                                            PtrConst::new(b),
                                        )
                                    };
                                    match ord {
                                        Some(core::cmp::Ordering::Equal) => {}
                                        Some(order) => return Some(order),
                                        None => return None,
                                    }
                                }

                                a.len.partial_cmp(&b.len)
                            });
                        }

                        if T::SHAPE.vtable.hash.is_some() {
                            builder = builder.hash(|value, state, hasher| {
                                let ptr = SlicePtr::<T>::new(value);
                                for item in ptr {
                                    unsafe {
                                        (T::SHAPE.vtable.hash.unwrap_unchecked())(
                                            PtrConst::new(item),
                                            state,
                                            hasher,
                                        )
                                    };
                                }
                            });
                        }
                    }

                    builder.build()
                },
            )
            .build()
    };
}
