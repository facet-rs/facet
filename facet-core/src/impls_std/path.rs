use crate::*;

unsafe impl Facet<'_> for std::path::PathBuf {
    const SHAPE: &'static Shape = &const {
        Shape {
            id: Shape::id_of::<Self>(),
            layout: Shape::layout_of::<Self>(),
            vtable: value_vtable!(std::path::PathBuf, |f, _opts| write!(
                f,
                "{}",
                Self::SHAPE.type_identifier
            )),
            ty: Type::User(UserType::Opaque),
            def: Def::Scalar,
            type_identifier: "PathBuf",
            type_params: &[],
            doc: &[],
            attributes: &[],
            type_tag: None,
            inner: None,
            proxy: None,
        }
    };
}

unsafe impl Facet<'_> for std::path::Path {
    const SHAPE: &'static Shape = &const {
        Shape {
            id: Shape::id_of::<Self>(),
            layout: Shape::UNSIZED_LAYOUT,
            vtable: value_vtable!(std::path::Path, |f, _opts| write!(
                f,
                "{}",
                Self::SHAPE.type_identifier
            )),
            ty: Type::User(UserType::Opaque),
            def: Def::Scalar,
            type_identifier: "Path",
            type_params: &[],
            doc: &[],
            attributes: &[],
            type_tag: None,
            inner: None,
            proxy: None,
        }
    };
}
