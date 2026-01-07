#![cfg(feature = "rust_decimal")]

use alloc::string::String;
use rust_decimal::Decimal;

use crate::{
    Def, Facet, OxPtrConst, OxPtrMut, ParseError, PtrConst, Shape, ShapeBuilder, TryFromOutcome,
    Type, UserType, VTableIndirect,
};

unsafe fn try_from_decimal(
    target: OxPtrMut,
    src_shape: &'static Shape,
    src: PtrConst,
) -> TryFromOutcome {
    unsafe {
        // Handle &str (Copy type, use get)
        if src_shape.id == <&str as Facet>::SHAPE.id {
            let source_str: &str = *src.get::<&str>();
            match source_str.parse::<Decimal>() {
                Ok(val) => {
                    *target.as_mut::<Decimal>() = val;
                    TryFromOutcome::Converted
                }
                Err(_) => TryFromOutcome::Failed("Decimal parsing failed".into()),
            }
        }
        // Handle String (consume via read)
        else if src_shape.id == <String as Facet>::SHAPE.id {
            let source_str = src.read::<String>();
            match source_str.parse::<Decimal>() {
                Ok(val) => {
                    *target.as_mut::<Decimal>() = val;
                    TryFromOutcome::Converted
                }
                Err(_) => TryFromOutcome::Failed("Decimal parsing failed".into()),
            }
        }
        // Note: We intentionally do NOT support f64/f32 conversion because it defeats
        // the purpose of Decimal (avoiding floating-point precision issues).
        // Formats should pass strings to Decimal::parse instead.
        else {
            TryFromOutcome::Unsupported
        }
    }
}

unsafe fn parse_decimal(s: &str, target: OxPtrMut) -> Option<Result<(), ParseError>> {
    unsafe {
        let parsed = s
            .parse::<Decimal>()
            .map_err(|_| ParseError::from_str("Decimal parsing failed"));
        Some(match parsed {
            Ok(val) => {
                *target.as_mut::<Decimal>() = val;
                Ok(())
            }
            Err(e) => Err(e),
        })
    }
}

unsafe fn display_decimal(
    source: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    unsafe {
        let decimal = source.get::<Decimal>();
        Some(write!(f, "{decimal}"))
    }
}

unsafe fn partial_eq_decimal(a: OxPtrConst, b: OxPtrConst) -> Option<bool> {
    unsafe {
        let a = a.get::<Decimal>();
        let b = b.get::<Decimal>();
        Some(a == b)
    }
}

unsafe fn partial_ord_decimal(a: OxPtrConst, b: OxPtrConst) -> Option<Option<core::cmp::Ordering>> {
    unsafe {
        let a = a.get::<Decimal>();
        let b = b.get::<Decimal>();
        Some(a.partial_cmp(b))
    }
}

unsafe fn ord_decimal(a: OxPtrConst, b: OxPtrConst) -> Option<core::cmp::Ordering> {
    unsafe {
        let a = a.get::<Decimal>();
        let b = b.get::<Decimal>();
        Some(a.cmp(b))
    }
}

unsafe fn hash_decimal(value: OxPtrConst, state: &mut crate::HashProxy<'_>) -> Option<()> {
    unsafe {
        use core::hash::Hash;
        let decimal = value.get::<Decimal>();
        decimal.hash(state);
        Some(())
    }
}

unsafe fn debug_decimal(
    source: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    unsafe {
        let decimal = source.get::<Decimal>();
        Some(write!(f, "{decimal:?}"))
    }
}

const DECIMAL_VTABLE: VTableIndirect = VTableIndirect {
    display: Some(display_decimal),
    debug: Some(debug_decimal),
    try_from: Some(try_from_decimal),
    parse: Some(parse_decimal),
    partial_eq: Some(partial_eq_decimal),
    partial_cmp: Some(partial_ord_decimal),
    cmp: Some(ord_decimal),
    hash: Some(hash_decimal),
    ..VTableIndirect::EMPTY
};

unsafe impl Facet<'_> for Decimal {
    const SHAPE: &'static Shape = &const {
        ShapeBuilder::for_sized::<Decimal>("Decimal")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_indirect(&DECIMAL_VTABLE)
            .eq()
            .copy()
            .send()
            .sync()
            .build()
    };
}
