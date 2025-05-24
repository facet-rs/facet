use crate::*;

unsafe impl<'a, T> Facet<'a> for [T]
where
    T: Facet<'a>,
{
    const VTABLE: &'static ValueVTable = &const {
        ValueVTable::builder_unsized::<Self>()
            .type_name(|f, opts| {
                if let Some(opts) = opts.for_children() {
                    write!(f, "[")?;
                    (T::SHAPE.vtable.type_name)(f, opts)?;
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
                if (T::SHAPE.vtable.debug)().is_some() {
                    Some(|value, f| {
                        write!(f, "[")?;
                        for (i, item) in value.iter().enumerate() {
                            if i > 0 {
                                write!(f, ", ")?;
                            }
                            (<VTableView<T>>::of().debug().unwrap())(item, f)?;
                        }
                        write!(f, "]")
                    })
                } else {
                    None
                }
            })
            .eq(|| {
                if (T::SHAPE.vtable.eq)().is_some() {
                    Some(|a, b| {
                        if a.len() != b.len() {
                            return false;
                        }
                        for (x, y) in a.iter().zip(b.iter()) {
                            if !(<VTableView<T>>::of().eq().unwrap())(x, y) {
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
                if (T::SHAPE.vtable.partial_ord)().is_some() {
                    Some(|a, b| {
                        for (x, y) in a.iter().zip(b.iter()) {
                            let ord = (<VTableView<T>>::of().partial_ord().unwrap())(x, y);
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
                if (T::SHAPE.vtable.ord)().is_some() {
                    Some(|a, b| {
                        for (x, y) in a.iter().zip(b.iter()) {
                            let ord = (<VTableView<T>>::of().ord().unwrap())(x, y);
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
                if (T::SHAPE.vtable.hash)().is_some() {
                    Some(|value, state, hasher| {
                        for item in value.iter() {
                            (<VTableView<T>>::of().hash().unwrap())(item, state, hasher);
                        }
                    })
                } else {
                    None
                }
            })
            .build()
    };

    const SHAPE: &'static Shape<'static> = &const {
        Shape::builder_for_unsized::<Self>()
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
                                    let slice = ptr.get::<&[T]>();
                                    slice.len()
                                })
                                .as_ptr(|ptr| unsafe {
                                    let slice = ptr.get::<&[T]>();
                                    PtrConst::new(slice.as_ptr())
                                })
                                .as_mut_ptr(|ptr| unsafe {
                                    let slice = ptr.as_mut::<&mut [T]>();
                                    PtrMut::new(slice.as_mut_ptr())
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
