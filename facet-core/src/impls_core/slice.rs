use core::ptr::NonNull;

use crate::*;

unsafe impl<'a, T> Facet<'a> for [T]
where
    T: Facet<'a>,
{
    const SHAPE: &'static Shape = &const {
        Shape::builder_for_unsized::<Self>()
            .vtable({
                ValueVTable::builder::<Self>()
                    .type_name(|f, opts| {
                        if let Some(opts) = opts.for_children() {
                            write!(f, "[")?;
                            (T::SHAPE.vtable.type_name())(f, opts)?;
                            write!(f, "]")
                        } else {
                            write!(f, "[⋯]")
                        }
                    })
                    .marker_traits(|| {
                        T::SHAPE
                            .vtable
                            .marker_traits()
                            .difference(MarkerTraits::COPY)
                    })
                    .debug(|| {
                        if T::SHAPE.vtable.has_debug() {
                            Some(|value, f| {
                                let value = value.get();
                                write!(f, "[")?;
                                for (i, item) in value.iter().enumerate() {
                                    if i > 0 {
                                        write!(f, ", ")?;
                                    }
                                    (<VTableView<T>>::of().debug().unwrap())(item.into(), f)?;
                                }
                                write!(f, "]")
                            })
                        } else {
                            None
                        }
                    })
                    .partial_eq(|| {
                        if T::SHAPE.vtable.has_partial_eq() {
                            Some(|a, b| {
                                let a = a.get();
                                let b = b.get();
                                if a.len() != b.len() {
                                    return false;
                                }
                                for (x, y) in a.iter().zip(b.iter()) {
                                    if !(<VTableView<T>>::of().partial_eq().unwrap())(
                                        x.into(),
                                        y.into(),
                                    ) {
                                        return false;
                                    }
                                }
                                true
                            })
                        } else {
                            None
                        }
                    })
                    .partial_ord(|| {
                        if T::SHAPE.vtable.has_partial_ord() {
                            Some(|a, b| {
                                let a = a.get();
                                let b = b.get();
                                for (x, y) in a.iter().zip(b.iter()) {
                                    let ord = (<VTableView<T>>::of().partial_ord().unwrap())(
                                        x.into(),
                                        y.into(),
                                    );
                                    match ord {
                                        Some(core::cmp::Ordering::Equal) => continue,
                                        Some(order) => return Some(order),
                                        None => return None,
                                    }
                                }
                                a.len().partial_cmp(&b.len())
                            })
                        } else {
                            None
                        }
                    })
                    .ord(|| {
                        if T::SHAPE.vtable.has_ord() {
                            Some(|a, b| {
                                let a = a.get();
                                let b = b.get();
                                for (x, y) in a.iter().zip(b.iter()) {
                                    let ord =
                                        (<VTableView<T>>::of().ord().unwrap())(x.into(), y.into());
                                    if ord != core::cmp::Ordering::Equal {
                                        return ord;
                                    }
                                }
                                a.len().cmp(&b.len())
                            })
                        } else {
                            None
                        }
                    })
                    .hash(|| {
                        if T::SHAPE.vtable.has_hash() {
                            Some(|value, hasher| {
                                for item in value.get().iter() {
                                    (<VTableView<T>>::of().hash().unwrap())(item.into(), hasher);
                                }
                            })
                        } else {
                            None
                        }
                    })
                    .build()
            })
            .type_identifier("[_]")
            .type_params(&[TypeParam {
                name: "T",
                shape: || T::SHAPE,
            }])
            .ty(Type::Sequence(SequenceType::Slice(SliceType {
                t: T::SHAPE,
            })))
            .def(Def::Slice(
                SliceDef::builder()
                    .vtable(
                        &const {
                            SliceVTable::builder()
                                .len(|ptr| unsafe {
                                    let slice = ptr.get::<[T]>();
                                    slice.len()
                                })
                                .as_ptr(|ptr| unsafe {
                                    let slice = ptr.get::<[T]>();
                                    PtrConst::new(NonNull::new_unchecked(slice.as_ptr() as *mut T))
                                })
                                .as_mut_ptr(|ptr| unsafe {
                                    let slice = ptr.as_mut::<[T]>();
                                    PtrMut::new(NonNull::new_unchecked(slice.as_mut_ptr()))
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
