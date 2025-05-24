use crate::*;

unsafe impl Facet<'_> for std::path::PathBuf {
    const VTABLE: &'static ValueVTable = &const {
        value_vtable!(std::path::PathBuf, |f, _opts| write!(
            f,
            "{}",
            Self::SHAPE.type_identifier
        ))
    };

    const SHAPE: &'static Shape<'static> = &const {
        Shape::builder_for_sized::<Self>()
            .type_identifier("PathBuf")
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
    const VTABLE: &'static ValueVTable = &const {
        value_vtable_unsized!(std::path::Path, |f, _opts| write!(
            f,
            "{}",
            Self::SHAPE.type_identifier
        ))
    };

    const SHAPE: &'static Shape<'static> = &const {
        Shape::builder_for_unsized::<Self>()
            .type_identifier("Path")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(&const { ScalarAffinity::path().build() })
                    .build(),
            ))
            .build()
    };
}
