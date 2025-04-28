use crate::*;

unsafe impl Facet<'_> for std::path::PathBuf {
    const SHAPE: &'static Shape = &const {
        Shape::builder_for_sized::<Self>()
            .ty(Type::User(UserType::opaque()))
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(ScalarAffinity::path().build())
                    .build(),
            ))
            .vtable(&const { value_vtable!((), |f, _opts| write!(f, "PathBuf")) })
            .build()
    };
}

unsafe impl Facet<'_> for std::path::Path {
    const SHAPE: &'static Shape = &const {
        Shape::builder_for_unsized::<Self>()
            .ty(Type::User(UserType::opaque()))
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(ScalarAffinity::path().build())
                    .build(),
            ))
            .vtable(&const { value_vtable!((), |f, _opts| write!(f, "Path")) })
            .build()
    };
}
