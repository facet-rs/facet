use crate::*;
use core::{cmp::Ordering, iter::zip};

unsafe impl<'a, T, const L: usize> Facet<'a> for [T; L]
where
    T: Facet<'a>,
{
    const VTABLE: &'static ValueVTable = &const {
        ValueVTable::builder::<Self>()
            .marker_traits(T::SHAPE.vtable.marker_traits)
            .type_name(|f, opts| {
                if let Some(opts) = opts.for_children() {
                    write!(f, "[")?;
                    (T::SHAPE.vtable.type_name)(f, opts)?;
                    write!(f, "; {L}]")
                } else {
                    write!(f, "[⋯; {L}]")
                }
            })
            .display(|| {
                if (T::SHAPE.vtable.display)().is_some() {
                    Some(|value, f| {
                        write!(f, "[")?;

                        for (idx, value) in value.iter().enumerate() {
                            (<VTableView<T>>::of().display().unwrap())(value, f)?;
                            if idx != L - 1 {
                                write!(f, ", ")?;
                            }
                        }
                        write!(f, "]")
                    })
                } else {
                    None
                }
            })
            .debug(|| {
                if (T::SHAPE.vtable.debug)().is_some() {
                    Some(|value, f| {
                        write!(f, "[")?;

                        for (idx, value) in value.iter().enumerate() {
                            (<VTableView<T>>::of().debug().unwrap())(value, f)?;
                            if idx != L - 1 {
                                write!(f, ", ")?;
                            }
                        }
                        write!(f, "]")
                    })
                } else {
                    None
                }
            })
            .eq(|| {
                if (T::SHAPE.vtable.eq)().is_some() {
                    Some(|a, b| zip(a, b).all(|(a, b)| (<VTableView<T>>::of().eq().unwrap())(a, b)))
                } else {
                    None
                }
            })
            .default_in_place(|| {
                if L == 0 {
                    // Zero-length arrays implement `Default` irrespective of the element type
                    Some(|target| unsafe { target.assume_init() })
                } else if L <= 32 && (T::SHAPE.vtable.default_in_place)().is_some() {
                    Some(|mut target| unsafe {
                        let t_dip = <VTableView<T>>::of().default_in_place().unwrap();
                        let stride = T::SHAPE
                            .layout
                            .sized_layout()
                            .unwrap()
                            .pad_to_align()
                            .size();
                        for idx in 0..L {
                            t_dip(target.field_uninit_at(idx * stride));
                        }
                        target.assume_init()
                    })
                } else {
                    // arrays do not yet implement `Default` for > 32 elements due
                    // to specializing the `0` len case
                    None
                }
            })
            .clone_into(|| {
                if (T::SHAPE.vtable.clone_into)().is_some() {
                    Some(|src, mut dst| unsafe {
                        let t_cip = <VTableView<T>>::of().clone_into().unwrap();
                        let stride = T::SHAPE
                            .layout
                            .sized_layout()
                            .unwrap()
                            .pad_to_align()
                            .size();
                        for (idx, src) in src.iter().enumerate() {
                            (t_cip)(src, dst.field_uninit_at(idx * stride));
                        }
                        dst.assume_init()
                    })
                } else {
                    None
                }
            })
            .partial_ord(|| {
                if (T::SHAPE.vtable.partial_ord)().is_some() {
                    Some(|a, b| {
                        zip(a, b)
                            .find_map(|(a, b)| {
                                match (<VTableView<T>>::of().partial_ord().unwrap())(a, b) {
                                    Some(Ordering::Equal) => None,
                                    c => Some(c),
                                }
                            })
                            .unwrap_or(Some(Ordering::Equal))
                    })
                } else {
                    // arrays do not yet implement `Default` for > 32 elements due
                    // to specializing the `0` len case
                    None
                }
            })
            .ord(|| {
                if (T::SHAPE.vtable.ord)().is_some() {
                    Some(|a, b| {
                        zip(a, b)
                            .find_map(
                                |(a, b)| match (<VTableView<T>>::of().ord().unwrap())(a, b) {
                                    Ordering::Equal => None,
                                    c => Some(c),
                                },
                            )
                            .unwrap_or(Ordering::Equal)
                    })
                } else {
                    // arrays do not yet implement `Default` for > 32 elements due
                    // to specializing the `0` len case
                    None
                }
            })
            .hash(|| {
                if (T::SHAPE.vtable.hash)().is_some() {
                    Some(|value, state, hasher| {
                        for value in value {
                            (<VTableView<T>>::of().hash().unwrap())(value, state, hasher)
                        }
                    })
                } else {
                    // arrays do not yet implement `Default` for > 32 elements due
                    // to specializing the `0` len case
                    None
                }
            })
            .build()
    };

    const SHAPE: &'static Shape<'static> = &const {
        Shape::builder_for_sized::<Self>()
            .type_identifier("&[_; _]")
            .type_params(&[TypeParam {
                name: "T",
                shape: || T::SHAPE,
            }])
            .ty(Type::Sequence(SequenceType::Array(ArrayType {
                t: T::SHAPE,
                n: L,
            })))
            .def(Def::Array(
                ArrayDef::builder()
                    .vtable(
                        &const {
                            ArrayVTable::builder()
                                .as_ptr(|ptr| unsafe {
                                    let array = ptr.get::<[T; L]>();
                                    PtrConst::new(array.as_ptr())
                                })
                                .as_mut_ptr(|ptr| unsafe {
                                    let array = ptr.as_mut::<[T; L]>();
                                    PtrMut::new(array.as_mut_ptr())
                                })
                                .build()
                        },
                    )
                    .t(T::SHAPE)
                    .n(L)
                    .build(),
            ))
            .build()
    };
}
