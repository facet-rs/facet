use crate::*;

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
                    let builder = ValueVTable::builder()
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

                    builder.build()
                },
            )
            .build()
    };
}
