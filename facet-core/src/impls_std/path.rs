use crate::*;

unsafe impl Facet<'_> for std::path::PathBuf {
    const SHAPE: &'static Shape = &const {
        Shape::builder_for_sized::<Self>()
            .vtable(value_vtable!(std::path::PathBuf, |f, _opts| write!(
                f,
                "{}",
                Self::SHAPE.type_identifier
            )))
            .type_identifier("PathBuf")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .build()
    };
}

unsafe impl Facet<'_> for std::path::Path {
    const SHAPE: &'static Shape = &const {
        Shape::builder_for_unsized::<Self>()
            .vtable(value_vtable!(std::path::Path, |f, _opts| write!(
                f,
                "{}",
                Self::SHAPE.type_identifier
            )))
            .type_identifier("Path")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .build()
    };
}
