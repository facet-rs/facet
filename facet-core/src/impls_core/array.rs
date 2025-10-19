use core::ptr::NonNull;

use crate::*;

unsafe impl<'a, T, const L: usize> Facet<'a> for [T; L]
where
    T: Facet<'a>,
{
    const VTABLE: &'static ValueVTable = &const {
        ValueVTable::builder::<Self>()
            .marker_traits(|| T::SHAPE.vtable.marker_traits())
            .type_name(|f, opts| {
                if let Some(opts) = opts.for_children() {
                    write!(f, "[")?;
                    (T::SHAPE.vtable.type_name())(f, opts)?;
                    write!(f, "; {L}]")
                } else {
                    write!(f, "[⋯; {L}]")
                }
            })
            .default_in_place(|| {
                if L == 0 {
                    // Zero-length arrays implement `Default` irrespective of the element type
                    Some(|target| unsafe { target.assume_init().into() })
                } else if L <= 32 && T::SHAPE.vtable.has_default_in_place() {
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
                        target.assume_init().into()
                    })
                } else {
                    // arrays do not yet implement `Default` for > 32 elements due
                    // to specializing the `0` len case
                    None
                }
            })
            .clone_into(|| {
                if T::SHAPE.vtable.has_clone_into() {
                    Some(|src, mut dst| unsafe {
                        let src = src.get();
                        let t_cip = <VTableView<T>>::of().clone_into().unwrap();
                        let stride = T::SHAPE
                            .layout
                            .sized_layout()
                            .unwrap()
                            .pad_to_align()
                            .size();
                        for (idx, src) in src.iter().enumerate() {
                            (t_cip)(src.into(), dst.field_uninit_at(idx * stride));
                        }
                        dst.assume_init().into()
                    })
                } else {
                    None
                }
            })
            .build()
    };

    const SHAPE: &'static Shape = &const {
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
                                    PtrConst::new(NonNull::from(array))
                                })
                                .as_mut_ptr(|ptr| unsafe {
                                    let array = ptr.as_mut::<[T; L]>();
                                    PtrMut::new(NonNull::from(array))
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
