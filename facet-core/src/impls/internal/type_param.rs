//! Facet implementation for TypeParam

use crate::{
    Def, Facet, Shape, ShapeBuilder, Type, TypeParam, UserType, VTableDirect, vtable_direct,
};

// TypeParam - treat as opaque (contains recursive &'static Shape)
unsafe impl Facet<'_> for TypeParam {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = vtable_direct!(TypeParam => Debug,);

        ShapeBuilder::for_sized::<TypeParam>("TypeParam")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_direct(&VTABLE)
            .copy()
            .send()
            .sync()
            .build()
    };
}
