use crate::Facet;
use crate::{
    Def, PrimitiveType, Shape, ShapeBuilder, TextualType, Type, TypeOpsDirect, VTableIndirect,
    type_ops_direct, vtable_direct, vtable_indirect,
};

// TypeOps lifted out - shared static (char has Default but not Clone as Copy type)
static CHAR_TYPE_OPS: TypeOpsDirect = type_ops_direct!(char => Default);

unsafe impl Facet<'_> for char {
    const SHAPE: &'static Shape = &const {
        const VTABLE: crate::VTableDirect = vtable_direct!(char =>
            FromStr,
            Display,
            Debug,
            Hash,
            PartialEq,
            PartialOrd,
            Ord,
        );

        ShapeBuilder::for_sized::<char>("char")
            .ty(Type::Primitive(PrimitiveType::Textual(TextualType::Char)))
            .def(Def::Scalar)
            .vtable_direct(&VTABLE)
            .type_ops_direct(&CHAR_TYPE_OPS)
            .eq()
            .copy()
            .send()
            .sync()
            .build()
    };
}

unsafe impl Facet<'_> for str {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableIndirect = vtable_indirect!(str =>
            Display,
            Debug,
            Hash,
            PartialEq,
            PartialOrd,
            Ord,
        );

        ShapeBuilder::for_unsized::<str>("str")
            .ty(Type::Primitive(PrimitiveType::Textual(TextualType::Str)))
            .def(Def::Scalar)
            .vtable_indirect(&VTABLE)
            .eq()
            .send()
            .sync()
            .build()
    };
}
