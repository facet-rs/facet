use ruint::{Bits, Uint};

use crate::{Def, Facet, Shape, Type, UserType, value_vtable};

unsafe impl<'facet, const BITS: usize, const LIMBS: usize> Facet<'facet> for Uint<BITS, LIMBS> {
    const SHAPE: &'static Shape = &const {
        Shape {
            id: Shape::id_of::<Self>(),
            layout: Shape::layout_of::<Self>(),
            vtable: value_vtable!(Uint<BITS, LIMBS>, |f, _opts| write!(
                f,
                "Uint<{BITS}, {LIMBS}>"
            )),
            ty: Type::User(UserType::Opaque),
            def: Def::Scalar,
            type_identifier: "Uint",
            type_params: &[],
            doc: &[],
            attributes: &[],
            type_tag: None,
            inner: None,
        }
    };
}

unsafe impl<'facet, const BITS: usize, const LIMBS: usize> Facet<'facet> for Bits<BITS, LIMBS> {
    const SHAPE: &'static Shape = &const {
        Shape {
            id: Shape::id_of::<Self>(),
            layout: Shape::layout_of::<Self>(),
            vtable: value_vtable!(Bits<BITS, LIMBS>, |f, _opts| write!(
                f,
                "Bits<{BITS}, {LIMBS}>"
            )),
            ty: Type::User(UserType::Opaque),
            def: Def::Scalar,
            type_identifier: "Bits",
            type_params: &[],
            doc: &[],
            attributes: &[],
            type_tag: None,
            inner: None,
        }
    };
}

#[cfg(test)]
mod tests {
    use ruint::aliases::{B128, U256};

    use crate::{Facet, Shape, ShapeLayout};

    #[test]
    fn test_uint() {
        const SHAPE: &Shape = U256::SHAPE;
        assert_eq!(SHAPE.type_identifier, "Uint");
        assert!(matches!(SHAPE.layout, ShapeLayout::Sized(..)));
        let layout = SHAPE.layout.sized_layout().unwrap();
        assert_eq!(layout.size(), 32); // 4 limbs with type u64 -> 32 bytes
        assert_eq!(layout.align(), 8);
    }

    #[test]
    fn test_bits() {
        const SHAPE: &Shape = B128::SHAPE;
        assert_eq!(SHAPE.type_identifier, "Bits");
        assert!(matches!(SHAPE.layout, ShapeLayout::Sized(..)));
        let layout = SHAPE.layout.sized_layout().unwrap();
        assert_eq!(layout.size(), 16); // 2 limbs with type u64 -> 16 bytes
        assert_eq!(layout.align(), 8);
    }
}
