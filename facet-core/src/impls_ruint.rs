use ruint::{Bits, Uint};

use crate::{Def, Facet, Shape, Type, UserType, value_vtable};

unsafe impl<'facet, const BITS: usize, const LIMBS: usize> Facet<'facet> for Uint<BITS, LIMBS> {
    const SHAPE: &'static Shape = &const {
        Shape::builder_for_sized::<Self>()
            .vtable(value_vtable!(Uint<BITS, LIMBS>, |f, _opts| write!(
                f,
                "Uint<{BITS}, {LIMBS}>"
            )))
            .type_identifier("Uint")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .build()
    };
}

unsafe impl<'facet, const BITS: usize, const LIMBS: usize> Facet<'facet> for Bits<BITS, LIMBS> {
    const SHAPE: &'static Shape = &const {
        Shape::builder_for_sized::<Self>()
            .vtable(value_vtable!(Bits<BITS, LIMBS>, |f, _opts| write!(
                f,
                "Bits<{BITS}, {LIMBS}>"
            )))
            .type_identifier("Bits")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .build()
    };
}
