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
                    (T::SHAPE.vtable.type_name())(f, opts)?;
                    write!(f, "]")
                } else {
                    write!(f, "[⋯]")
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
