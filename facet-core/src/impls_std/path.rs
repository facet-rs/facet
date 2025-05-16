use crate::*;

unsafe impl Facet<'_> for std::path::PathBuf {
    const VTABLE: &'static ValueVTable =
        &const { value_vtable!((), |f, _opts| write!(f, "PathBuf")) };

    const SHAPE: &'static Shape<'static> = &const {
        Shape::builder_for_sized::<Self>()
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(&const { ScalarAffinity::path().build() })
                    .build(),
            ))
            .build()
    };
}

unsafe impl Facet<'_> for std::path::Path {
    const VTABLE: &'static ValueVTable = &const { value_vtable!((), |f, _opts| write!(f, "Path")) };

    const SHAPE: &'static Shape<'static> = &const {
        Shape::builder_for_unsized::<Self>()
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(&const { ScalarAffinity::path().build() })
                    .build(),
            ))
            .build()
    };
}
