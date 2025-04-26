use crate::*;
use core::{cmp::Ordering, iter::zip};

unsafe impl<'a, T, const L: usize> Facet<'a> for [T; L]
where
    T: Facet<'a>,
{
    const SHAPE: &'static Shape = &const {
        Shape::builder_for_sized::<Self>()
            .type_params(&[
                TypeParam {
                    name: "T",
                    shape: || T::SHAPE,
                }
            ])
            .vtable(
                &const {
                    let mut builder = ValueVTable::builder::<Self>()
                        .marker_traits(T::SHAPE.vtable.marker_traits)
                        .type_name(|f, opts| {
                            if let Some(opts) = opts.for_children() {
                                write!(f, "[")?;
                                (T::SHAPE.vtable.type_name)(f, opts)?;
                                write!(f, "; {L}]")
                            } else {
                                write!(f, "[⋯; {L}]")
                            }
                        });
                    if T::SHAPE.vtable.display.is_some() {
                        builder = builder.display(|value, f| {
                            let value = unsafe { value.get::<[T; L]>() };
                            write!(f, "[")?;

                            for (idx, value) in value.iter().enumerate() {
                                unsafe {
                                    (T::SHAPE.vtable.display.unwrap_unchecked())(
                                        PtrConst::new(value),
                                        f,
                                    )?
                                };
                                if idx != L - 1 {
                                    write!(f, ", ")?;
                                }
                            }
                            write!(f, "]")
                        });
                    }
                    if T::SHAPE.vtable.debug.is_some() {
                        builder = builder.debug(|value, f| {
                            let value = unsafe { value.get::<[T; L]>() };
                            write!(f, "[")?;

                            for (idx, value) in value.iter().enumerate() {
                                unsafe {
                                    (T::SHAPE.vtable.debug.unwrap_unchecked())(
                                        PtrConst::new(value),
                                        f,
                                    )?
                                };
                                if idx != L - 1 {
                                    write!(f, ", ")?;
                                }
                            }
                            write!(f, "]")
                        });
                    }
                    if T::SHAPE.vtable.eq.is_some() {
                        builder = builder.eq(|a, b| {
                            let a = unsafe { a.get::<[T; L]>() };
                            let b = unsafe { b.get::<[T; L]>() };
                            zip(a, b).all(|(a, b)| unsafe {
                                (T::SHAPE.vtable.eq.unwrap_unchecked())(
                                    PtrConst::new(a),
                                    PtrConst::new(b),
                                )
                            })
                        });
                    }
                    if L == 0 {
                        // Zero-length arrays implement `Default` irrespective of the element type
                        builder =
                            builder.default_in_place(|target| unsafe { target.assume_init() });
                    } else if L <= 32 && T::SHAPE.vtable.default_in_place.is_some() {
                        builder = builder.default_in_place(|target| unsafe {
                            let t_dip = T::SHAPE.vtable.default_in_place.unwrap_unchecked();
                            let stride = T::SHAPE.layout.sized_layout().unwrap().pad_to_align().size();
                            for idx in 0..L {
                                t_dip(target.field_uninit_at(idx * stride));
                            }
                            target.assume_init()
                        });
                    } else {
                        // arrays do not yet implement `Default` for > 32 elements due to
                        // specializing the `0` len case
                    }
                    if T::SHAPE.vtable.clone_into.is_some() {
                        builder = builder.clone_into(|src, dst| unsafe {
                            let t_cip = T::SHAPE.vtable.clone_into.unwrap_unchecked();
                            let src = src.get::<[T; L]>();
                            let stride = T::SHAPE.layout.sized_layout().unwrap().pad_to_align().size();
                            for (idx, src) in src.iter().enumerate() {
                                (t_cip)(PtrConst::new(src), dst.field_uninit_at(idx * stride));
                            }
                            dst.assume_init()
                        });
                    }
                    if T::SHAPE.vtable.partial_ord.is_some() {
                        builder = builder.partial_ord(|a, b| {
                            let a = unsafe { a.get::<[T; L]>() };
                            let b = unsafe { b.get::<[T; L]>() };
                            zip(a, b)
                                .find_map(|(a, b)| unsafe {
                                    match (T::SHAPE.vtable.partial_ord.unwrap_unchecked())(
                                        PtrConst::new(a),
                                        PtrConst::new(b),
                                    ) {
                                        Some(Ordering::Equal) => None,
                                        c => Some(c),
                                    }
                                })
                                .unwrap_or(Some(Ordering::Equal))
                        });
                    }
                    if T::SHAPE.vtable.ord.is_some() {
                        builder = builder.ord(|a, b| {
                            let a = unsafe { a.get::<[T; L]>() };
                            let b = unsafe { b.get::<[T; L]>() };
                            zip(a, b)
                                .find_map(|(a, b)| unsafe {
                                    match (T::SHAPE.vtable.ord.unwrap_unchecked())(
                                        PtrConst::new(a),
                                        PtrConst::new(b),
                                    ) {
                                        Ordering::Equal => None,
                                        c => Some(c),
                                    }
                                })
                                .unwrap_or(Ordering::Equal)
                        });
                    }
                    if T::SHAPE.vtable.hash.is_some() {
                        builder = builder.hash(|value, state, hasher| {
                            let value = unsafe { value.get::<[T; L]>() };
                            for value in value {
                                unsafe {
                                    (T::SHAPE.vtable.hash.unwrap_unchecked())(
                                        PtrConst::new(value),
                                        state,
                                        hasher,
                                    )
                                }
                            }
                        });
                    }
                    builder.build()
                },
            )
            .def(Def::Array(
                ArrayDef::builder()
                    .vtable(
                        &const {
                            ArrayVTable::builder()
                                .get_item_ptr(|ptr, index| unsafe {
                                    if index >= L {
                                        panic!(
                                            "Index out of bounds: the len is {L} but the index is {index}"
                                        );
                                    }
                                    PtrConst::new(ptr.as_ptr::<[T; L]>())
                                })
                                .build()
                        },
                    )
                    .n(L)
                    .t(|| T::SHAPE)
                    .build(),
            ))
            .build()
    };
}
