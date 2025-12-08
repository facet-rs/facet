//! Facet implementation for Shape

use crate::{Def, Facet, Shape, ShapeBuilder, Type, UserType, VTableDirect, vtable_direct};

// Shape - treat as opaque for now
// It contains many reference types (&'static str, &'static [T], etc.)
// that would require implementing Facet for those reference types.
// We can expand this later when we have proper support for reference types.
unsafe impl Facet<'_> for Shape {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = vtable_direct!(Shape =>
            Debug,
            Hash,
            PartialEq,
            PartialOrd,
            Ord,
        );

        ShapeBuilder::for_sized::<Shape>("Shape")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_direct(&VTABLE)
            .eq()
            .send()
            .sync()
            .build()
    };
}
