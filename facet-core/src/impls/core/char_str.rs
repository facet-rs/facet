use crate::Facet;
use crate::{
    Def, OxPtrMut, PrimitiveType, PtrConst, Shape, ShapeBuilder, TextualType, Type, TypeOpsDirect,
    TypeOpsIndirect, VTableIndirect, type_ops_direct, vtable_direct, vtable_indirect,
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
            .decl_id_prim()
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

#[inline(always)]
unsafe fn str_truthy(value: PtrConst) -> bool {
    !unsafe { value.get::<str>() }.is_empty()
}

unsafe fn str_drop(_: OxPtrMut) {}

static STR_TYPE_OPS: TypeOpsIndirect = TypeOpsIndirect {
    drop_in_place: str_drop,
    default_in_place: None,
    clone_into: None,
    is_truthy: Some(str_truthy),
};

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
            .decl_id_prim()
            .ty(Type::Primitive(PrimitiveType::Textual(TextualType::Str)))
            .def(Def::Scalar)
            .vtable_indirect(&VTABLE)
            .type_ops_indirect(&STR_TYPE_OPS)
            .eq()
            .send()
            .sync()
            .build()
    };
}
