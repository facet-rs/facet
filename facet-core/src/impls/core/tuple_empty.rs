use crate::{
    Def, Facet, Repr, Shape, ShapeBuilder, StructKind, StructType, Type, TypeOpsDirect, UserType,
    VTableDirect, type_ops_direct, vtable_direct,
};

// TypeOps lifted out - shared static (unit has Default but not Clone as Copy type)
static UNIT_TYPE_OPS: TypeOpsDirect = type_ops_direct!(() => Default);

unsafe impl Facet<'_> for () {
    const SHAPE: &'static Shape = &const {
        // () implements Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash
        // but NOT Display or FromStr

        const VTABLE: VTableDirect = vtable_direct!(() =>
            Debug,
            Hash,
            PartialEq,
            PartialOrd,
            Ord,
        );

        ShapeBuilder::for_sized::<()>("()")
            .decl_id_prim()
            .ty(Type::User(UserType::Struct(StructType {
                repr: Repr::default(),
                kind: StructKind::Tuple,
                fields: &[],
            })))
            .def(Def::Scalar)
            .vtable_direct(&VTABLE)
            .type_ops_direct(&UNIT_TYPE_OPS)
            .eq()
            .copy()
            .send()
            .sync()
            .build()
    };
}
