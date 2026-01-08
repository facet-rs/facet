//! Facet implementation for Type and related types

use crate::{Def, Facet, Shape, ShapeBuilder, Type, UserType, VTableDirect, vtable_direct};

// Type - treat as opaque for now (complex nested enum with recursive references to Shape)
unsafe impl Facet<'_> for Type {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = vtable_direct!(Type => Debug,);

        ShapeBuilder::for_sized::<Type>("Type")
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

// PrimitiveType - treat as opaque
unsafe impl Facet<'_> for crate::PrimitiveType {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = vtable_direct!(crate::PrimitiveType => Debug,);

        ShapeBuilder::for_sized::<crate::PrimitiveType>("PrimitiveType")
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

// NumericType - treat as opaque
unsafe impl Facet<'_> for crate::NumericType {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = vtable_direct!(crate::NumericType => Debug,);

        ShapeBuilder::for_sized::<crate::NumericType>("NumericType")
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

// TextualType - treat as opaque
unsafe impl Facet<'_> for crate::TextualType {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = vtable_direct!(crate::TextualType => Debug,);

        ShapeBuilder::for_sized::<crate::TextualType>("TextualType")
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

// SequenceType - treat as opaque
unsafe impl Facet<'_> for crate::SequenceType {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = vtable_direct!(crate::SequenceType => Debug,);

        ShapeBuilder::for_sized::<crate::SequenceType>("SequenceType")
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

// ArrayType - treat as opaque
unsafe impl Facet<'_> for crate::ArrayType {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = vtable_direct!(crate::ArrayType => Debug,);

        ShapeBuilder::for_sized::<crate::ArrayType>("ArrayType")
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

// SliceType - treat as opaque
unsafe impl Facet<'_> for crate::SliceType {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = vtable_direct!(crate::SliceType => Debug,);

        ShapeBuilder::for_sized::<crate::SliceType>("SliceType")
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

// UserType - treat as opaque
unsafe impl Facet<'_> for crate::UserType {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = vtable_direct!(crate::UserType => Debug,);

        ShapeBuilder::for_sized::<crate::UserType>("UserType")
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

// StructType - treat as opaque
unsafe impl Facet<'_> for crate::StructType {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = vtable_direct!(crate::StructType => Debug,);

        ShapeBuilder::for_sized::<crate::StructType>("StructType")
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

// EnumType - treat as opaque
unsafe impl Facet<'_> for crate::EnumType {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = vtable_direct!(crate::EnumType => Debug,);

        ShapeBuilder::for_sized::<crate::EnumType>("EnumType")
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

// UnionType - treat as opaque
unsafe impl Facet<'_> for crate::UnionType {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = vtable_direct!(crate::UnionType => Debug,);

        ShapeBuilder::for_sized::<crate::UnionType>("UnionType")
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

// PointerType - treat as opaque
unsafe impl Facet<'_> for crate::PointerType {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = vtable_direct!(crate::PointerType => Debug,);

        ShapeBuilder::for_sized::<crate::PointerType>("PointerType")
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

// ValuePointerType - treat as opaque
unsafe impl Facet<'_> for crate::ValuePointerType {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = vtable_direct!(crate::ValuePointerType => Debug,);

        ShapeBuilder::for_sized::<crate::ValuePointerType>("ValuePointerType")
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

// FunctionPointerDef - treat as opaque
unsafe impl Facet<'_> for crate::FunctionPointerDef {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = vtable_direct!(crate::FunctionPointerDef => Debug,);

        ShapeBuilder::for_sized::<crate::FunctionPointerDef>("FunctionPointerDef")
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

// Repr - treat as opaque
unsafe impl Facet<'_> for crate::Repr {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = vtable_direct!(crate::Repr => Debug, Hash, PartialEq,);

        ShapeBuilder::for_sized::<crate::Repr>("Repr")
            .decl_id_prim()
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_direct(&VTABLE)
            .copy()
            .send()
            .sync()
            .eq()
            .build()
    };
}

// BaseRepr - treat as opaque
unsafe impl Facet<'_> for crate::BaseRepr {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = vtable_direct!(crate::BaseRepr => Debug, Hash, PartialEq,);

        ShapeBuilder::for_sized::<crate::BaseRepr>("BaseRepr")
            .decl_id_prim()
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_direct(&VTABLE)
            .copy()
            .send()
            .sync()
            .eq()
            .build()
    };
}
