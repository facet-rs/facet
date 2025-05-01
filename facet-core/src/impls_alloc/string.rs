use crate::{Def, Facet, ScalarAffinity, ScalarDef, Shape, value_vtable};

unsafe impl Facet<'_> for alloc::string::String {
    const SHAPE: &'static Shape = &const {
        Shape::builder_for_sized::<Self>()
            .def(Def::Scalar(
                ScalarDef::builder()
                    // `String` is always on the heap
                    .affinity(ScalarAffinity::string().max_inline_length(0).build())
                    .build(),
            ))
            .vtable(&const { value_vtable!(alloc::string::String, |f, _opts| write!(f, "String")) })
            .build()
    };
}

unsafe impl<'a> Facet<'a> for alloc::borrow::Cow<'a, str> {
    const SHAPE: &'static Shape = &const {
        Shape::builder_for_sized::<Self>()
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(ScalarAffinity::string().build())
                    .build(),
            ))
            .vtable(
                &const {
                    value_vtable!(alloc::borrow::Cow<'_, str>, |f, _opts| write!(
                        f,
                        "Cow<'_, str>"
                    ))
                },
            )
            .build()
    };
}
