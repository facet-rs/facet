#![cfg(feature = "rust_decimal")]

use rust_decimal::Decimal;

use crate::{
    Def, Facet, Shape, ShapeBuilder, Type, TypeOpsDirect, UserType, VTableDirect, type_ops_direct,
    vtable_direct,
};

unsafe impl Facet<'_> for Decimal {
    const SHAPE: &'static Shape = &const {
        // Decimal implements Display, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, FromStr
        // It also implements Clone, Copy, Default
        const VTABLE: VTableDirect =
            vtable_direct!(Decimal => FromStr, Display, Debug, Hash, PartialEq, PartialOrd, Ord,);
        const TYPE_OPS: TypeOpsDirect = type_ops_direct!(Decimal => Default, Clone);

        ShapeBuilder::for_sized::<Decimal>("Decimal")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_direct(&VTABLE)
            .type_ops_direct(&TYPE_OPS)
            .eq()
            .copy()
            .send()
            .sync()
            .build()
    };
}
