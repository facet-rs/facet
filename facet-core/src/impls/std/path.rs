use crate::{
    Def, Facet, Shape, ShapeBuilder, Type, UserType, VTableDirect, VTableIndirect, vtable_direct,
    vtable_indirect,
};

unsafe impl Facet<'_> for std::path::PathBuf {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = vtable_direct!(std::path::PathBuf =>
            Debug,
            Hash,
            PartialEq,
            PartialOrd,
            Ord,
        );

        ShapeBuilder::for_sized::<std::path::PathBuf>("PathBuf")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_direct(&VTABLE)
            .eq()
            .send()
            .sync()
            .build()
    };
}

unsafe impl Facet<'_> for std::path::Path {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableIndirect = vtable_indirect!(std::path::Path =>
            Debug,
            Hash,
            PartialEq,
            PartialOrd,
            Ord,
        );

        ShapeBuilder::for_unsized::<std::path::Path>("Path")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_indirect(&VTABLE)
            .eq()
            .send()
            .sync()
            .build()
    };
}
