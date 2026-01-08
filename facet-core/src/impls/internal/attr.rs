//! Facet implementation for Attr

use crate::{Attr, Def, Facet, Shape, ShapeBuilder, Type, UserType, VTableDirect, vtable_direct};

// Attr - treat as opaque (contains OxRef<'static> which is complex)
unsafe impl Facet<'_> for Attr {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = vtable_direct!(Attr => Debug, PartialEq,);

        ShapeBuilder::for_sized::<Attr>("Attr")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_direct(&VTABLE)
            .send()
            .sync()
            .build()
    };
}
