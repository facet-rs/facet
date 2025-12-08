//! Facet implementation for Def and related types

use crate::{Def, Facet, Shape, ShapeBuilder, Type, UserType, VTableDirect, vtable_direct};

// Def - treat as opaque (complex enum with many variants containing recursive Shape references)
unsafe impl Facet<'_> for Def {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = vtable_direct!(Def => Debug,);

        ShapeBuilder::for_sized::<Def>("Def")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_direct(&VTABLE)
            .copy()
            .send()
            .sync()
            .build()
    };
}

// MapDef - treat as opaque
unsafe impl Facet<'_> for crate::MapDef {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = vtable_direct!(crate::MapDef => Debug,);

        ShapeBuilder::for_sized::<crate::MapDef>("MapDef")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_direct(&VTABLE)
            .copy()
            .send()
            .sync()
            .build()
    };
}

// SetDef - treat as opaque
unsafe impl Facet<'_> for crate::SetDef {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = vtable_direct!(crate::SetDef => Debug,);

        ShapeBuilder::for_sized::<crate::SetDef>("SetDef")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_direct(&VTABLE)
            .copy()
            .send()
            .sync()
            .build()
    };
}

// ListDef - treat as opaque
unsafe impl Facet<'_> for crate::ListDef {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = vtable_direct!(crate::ListDef => Debug,);

        ShapeBuilder::for_sized::<crate::ListDef>("ListDef")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_direct(&VTABLE)
            .copy()
            .send()
            .sync()
            .build()
    };
}

// ArrayDef - treat as opaque
unsafe impl Facet<'_> for crate::ArrayDef {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = vtable_direct!(crate::ArrayDef => Debug,);

        ShapeBuilder::for_sized::<crate::ArrayDef>("ArrayDef")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_direct(&VTABLE)
            .copy()
            .send()
            .sync()
            .build()
    };
}

// NdArrayDef - treat as opaque
unsafe impl Facet<'_> for crate::NdArrayDef {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = vtable_direct!(crate::NdArrayDef => Debug,);

        ShapeBuilder::for_sized::<crate::NdArrayDef>("NdArrayDef")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_direct(&VTABLE)
            .copy()
            .send()
            .sync()
            .build()
    };
}

// SliceDef - treat as opaque
unsafe impl Facet<'_> for crate::SliceDef {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = vtable_direct!(crate::SliceDef => Debug,);

        ShapeBuilder::for_sized::<crate::SliceDef>("SliceDef")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_direct(&VTABLE)
            .copy()
            .send()
            .sync()
            .build()
    };
}

// OptionDef - treat as opaque
unsafe impl Facet<'_> for crate::OptionDef {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = vtable_direct!(crate::OptionDef => Debug,);

        ShapeBuilder::for_sized::<crate::OptionDef>("OptionDef")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_direct(&VTABLE)
            .copy()
            .send()
            .sync()
            .build()
    };
}

// ResultDef - treat as opaque
unsafe impl Facet<'_> for crate::ResultDef {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = vtable_direct!(crate::ResultDef => Debug,);

        ShapeBuilder::for_sized::<crate::ResultDef>("ResultDef")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_direct(&VTABLE)
            .copy()
            .send()
            .sync()
            .build()
    };
}

// PointerDef - treat as opaque
unsafe impl Facet<'_> for crate::PointerDef {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = vtable_direct!(crate::PointerDef => Debug,);

        ShapeBuilder::for_sized::<crate::PointerDef>("PointerDef")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_direct(&VTABLE)
            .copy()
            .send()
            .sync()
            .build()
    };
}

// DynamicValueDef - treat as opaque
unsafe impl Facet<'_> for crate::DynamicValueDef {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = vtable_direct!(crate::DynamicValueDef => Debug,);

        ShapeBuilder::for_sized::<crate::DynamicValueDef>("DynamicValueDef")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_direct(&VTABLE)
            .copy()
            .send()
            .sync()
            .build()
    };
}
