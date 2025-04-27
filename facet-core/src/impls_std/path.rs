use crate::*;

unsafe impl Facet<'_> for std::path::PathBuf {
    const SHAPE: &'static Shape = &const {
        Shape::builder_for_sized::<Self>()
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(ScalarAffinity::path().build())
                    .build(),
            ))
            .vtable(&const { value_vtable!((), |f, _opts| write!(f, "PathBuf")) })
            .build()
    };
}

unsafe impl<'a> Facet<'a> for &'a std::path::Path {
    const SHAPE: &'static Shape = &const {
        Shape::builder_for_sized::<Self>()
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(ScalarAffinity::path().build())
                    .build(),
            ))
            .vtable(&const { value_vtable!((), |f, _opts| write!(f, "Path")) })
            .build()
    };
}
