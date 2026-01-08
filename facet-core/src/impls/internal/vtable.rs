//! Facet implementation for VTableErased

use crate::{
    Def, Facet, Shape, ShapeBuilder, Type, UserType, VTableDirect, VTableErased, vtable_direct,
};

unsafe impl Facet<'_> for VTableErased {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = vtable_direct!(VTableErased =>
            Debug,
        );

        // VTableErased is an enum but we treat it as opaque
        // since its variants contain function pointers
        ShapeBuilder::for_sized::<VTableErased>("VTableErased")
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
