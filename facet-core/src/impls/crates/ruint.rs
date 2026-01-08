#![cfg(feature = "ruint")]

use ruint::{Bits, Uint};

use crate::{
    Def, Facet, HashProxy, OxPtrConst, Shape, ShapeBuilder, Type, UserType, VTableIndirect,
};

/// Debug for Uint<BITS, LIMBS>
unsafe fn uint_debug<const BITS: usize, const LIMBS: usize>(
    source: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    let value = unsafe { source.get::<Uint<BITS, LIMBS>>() };
    Some(core::fmt::Debug::fmt(value, f))
}

/// Hash for Uint<BITS, LIMBS>
unsafe fn uint_hash<const BITS: usize, const LIMBS: usize>(
    source: OxPtrConst,
    hasher: &mut HashProxy<'_>,
) -> Option<()> {
    use core::hash::Hash;
    let value = unsafe { source.get::<Uint<BITS, LIMBS>>() };
    value.hash(hasher);
    Some(())
}

/// PartialEq for Uint<BITS, LIMBS>
unsafe fn uint_partial_eq<const BITS: usize, const LIMBS: usize>(
    a: OxPtrConst,
    b: OxPtrConst,
) -> Option<bool> {
    let a_val = unsafe { a.get::<Uint<BITS, LIMBS>>() };
    let b_val = unsafe { b.get::<Uint<BITS, LIMBS>>() };
    Some(a_val == b_val)
}

/// PartialOrd for Uint<BITS, LIMBS>
unsafe fn uint_partial_cmp<const BITS: usize, const LIMBS: usize>(
    a: OxPtrConst,
    b: OxPtrConst,
) -> Option<Option<core::cmp::Ordering>> {
    let a_val = unsafe { a.get::<Uint<BITS, LIMBS>>() };
    let b_val = unsafe { b.get::<Uint<BITS, LIMBS>>() };
    Some(a_val.partial_cmp(b_val))
}

/// Ord for Uint<BITS, LIMBS>
unsafe fn uint_cmp<const BITS: usize, const LIMBS: usize>(
    a: OxPtrConst,
    b: OxPtrConst,
) -> Option<core::cmp::Ordering> {
    let a_val = unsafe { a.get::<Uint<BITS, LIMBS>>() };
    let b_val = unsafe { b.get::<Uint<BITS, LIMBS>>() };
    Some(a_val.cmp(b_val))
}

unsafe impl<'facet, const BITS: usize, const LIMBS: usize> Facet<'facet> for Uint<BITS, LIMBS> {
    const SHAPE: &'static Shape = &const {
        const fn build_uint_vtable<const BITS: usize, const LIMBS: usize>() -> VTableIndirect {
            VTableIndirect {
                debug: Some(uint_debug::<BITS, LIMBS>),
                hash: Some(uint_hash::<BITS, LIMBS>),
                partial_eq: Some(uint_partial_eq::<BITS, LIMBS>),
                partial_cmp: Some(uint_partial_cmp::<BITS, LIMBS>),
                cmp: Some(uint_cmp::<BITS, LIMBS>),
                ..VTableIndirect::EMPTY
            }
        }

        ShapeBuilder::for_sized::<Self>("Uint")
            .decl_id_prim()
            .module_path("ruint")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_indirect(&const { build_uint_vtable::<BITS, LIMBS>() })
            .build()
    };
}

/// Debug for Bits<BITS, LIMBS>
unsafe fn bits_debug<const BITS: usize, const LIMBS: usize>(
    source: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    let value = unsafe { source.get::<Bits<BITS, LIMBS>>() };
    Some(core::fmt::Debug::fmt(value, f))
}

/// Hash for Bits<BITS, LIMBS>
unsafe fn bits_hash<const BITS: usize, const LIMBS: usize>(
    source: OxPtrConst,
    hasher: &mut HashProxy<'_>,
) -> Option<()> {
    use core::hash::Hash;
    let value = unsafe { source.get::<Bits<BITS, LIMBS>>() };
    value.hash(hasher);
    Some(())
}

/// PartialEq for Bits<BITS, LIMBS>
unsafe fn bits_partial_eq<const BITS: usize, const LIMBS: usize>(
    a: OxPtrConst,
    b: OxPtrConst,
) -> Option<bool> {
    let a_val = unsafe { a.get::<Bits<BITS, LIMBS>>() };
    let b_val = unsafe { b.get::<Bits<BITS, LIMBS>>() };
    Some(a_val == b_val)
}

unsafe impl<'facet, const BITS: usize, const LIMBS: usize> Facet<'facet> for Bits<BITS, LIMBS> {
    const SHAPE: &'static Shape = &const {
        const fn build_bits_vtable<const BITS: usize, const LIMBS: usize>() -> VTableIndirect {
            VTableIndirect {
                debug: Some(bits_debug::<BITS, LIMBS>),
                hash: Some(bits_hash::<BITS, LIMBS>),
                partial_eq: Some(bits_partial_eq::<BITS, LIMBS>),
                ..VTableIndirect::EMPTY
            }
        }

        ShapeBuilder::for_sized::<Self>("Bits")
            .decl_id_prim()
            .module_path("ruint")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_indirect(&const { build_bits_vtable::<BITS, LIMBS>() })
            .build()
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
