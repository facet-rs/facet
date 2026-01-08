//! Facet implementation for ShapeLayout

use crate::{
    Def, Facet, Shape, ShapeBuilder, ShapeLayout, Type, UserType, VTableDirect, vtable_direct,
};

unsafe impl Facet<'_> for ShapeLayout {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = vtable_direct!(ShapeLayout =>
            Debug,
            Hash,
        );

        // ShapeLayout is an enum but we treat it as opaque for now
        // since Layout doesn't have a Facet impl
        ShapeBuilder::for_sized::<ShapeLayout>("ShapeLayout")
            .decl_id_prim()
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_direct(&VTABLE)
            .copy()
            .send()
            .sync()
            .build()
    };
}
