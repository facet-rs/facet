//! Facet implementation for MarkerTraits

use crate::{
    Def, Facet, MarkerTraits, Shape, ShapeBuilder, Type, UserType, VTableDirect, vtable_direct,
};

unsafe impl Facet<'_> for MarkerTraits {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = vtable_direct!(MarkerTraits =>
            Debug,
            Hash,
            PartialEq,
        );

        ShapeBuilder::for_sized::<MarkerTraits>("MarkerTraits")
            .decl_id_prim()
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_direct(&VTABLE)
            .eq()
            .copy()
            .send()
            .sync()
            .build()
    };
}
