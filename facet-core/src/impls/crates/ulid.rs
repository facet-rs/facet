#![cfg(feature = "ulid")]

use alloc::string::String;

use ulid::Ulid;

use crate::{
    Def, Facet, OxPtrConst, OxPtrMut, ParseError, PtrConst, Shape, ShapeBuilder, TryFromOutcome,
    Type, UserType, VTableIndirect,
};

unsafe fn try_from_ulid(
    target: OxPtrMut,
    src_shape: &'static Shape,
    src: PtrConst,
) -> TryFromOutcome {
    unsafe {
        // Handle &str (Copy type, use get)
        if src_shape.id == <&str as Facet>::SHAPE.id {
            let source_str: &str = src.get::<&str>();
            match Ulid::from_string(source_str) {
                Ok(val) => {
                    *target.as_mut::<Ulid>() = val;
                    TryFromOutcome::Converted
                }
                Err(_) => TryFromOutcome::Failed("ULID parsing failed".into()),
            }
        }
        // Handle String (consume via read)
        else if src_shape.id == <String as Facet>::SHAPE.id {
            let source_str = src.read::<String>();
            match Ulid::from_string(&source_str) {
                Ok(val) => {
                    *target.as_mut::<Ulid>() = val;
                    TryFromOutcome::Converted
                }
                Err(_) => TryFromOutcome::Failed("ULID parsing failed".into()),
            }
        } else {
            TryFromOutcome::Unsupported
        }
    }
}

unsafe fn parse_ulid(s: &str, target: OxPtrMut) -> Option<Result<(), ParseError>> {
    unsafe {
        let parsed = Ulid::from_string(s).map_err(|_| ParseError::from_str("ULID parsing failed"));
        Some(match parsed {
            Ok(val) => {
                *target.as_mut::<Ulid>() = val;
                Ok(())
            }
            Err(e) => Err(e),
        })
    }
}

unsafe fn display_ulid(
    source: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    unsafe {
        let ulid = source.get::<Ulid>();
        Some(write!(f, "{ulid}"))
    }
}

unsafe fn partial_eq_ulid(a: OxPtrConst, b: OxPtrConst) -> Option<bool> {
    unsafe {
        let a = a.get::<Ulid>();
        let b = b.get::<Ulid>();
        Some(a == b)
    }
}

unsafe fn parse_bytes_ulid(bytes: &[u8], target: OxPtrMut) -> Option<Result<(), ParseError>> {
    unsafe {
        if bytes.len() != 16 {
            return Some(Err(ParseError::from_str("ULID must be exactly 16 bytes")));
        }
        // Convert bytes to u128 (big-endian, which is how ULID is serialized)
        let mut arr = [0u8; 16];
        arr.copy_from_slice(bytes);
        let val = u128::from_be_bytes(arr);
        *target.as_mut::<Ulid>() = Ulid::from(val);
        Some(Ok(()))
    }
}

const ULID_VTABLE: VTableIndirect = VTableIndirect {
    display: Some(display_ulid),
    try_from: Some(try_from_ulid),
    parse: Some(parse_ulid),
    parse_bytes: Some(parse_bytes_ulid),
    partial_eq: Some(partial_eq_ulid),
    ..VTableIndirect::EMPTY
};

unsafe impl Facet<'_> for Ulid {
    const SHAPE: &'static Shape = &const {
        ShapeBuilder::for_sized::<Ulid>("Ulid")
            .module_path("ulid")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_indirect(&ULID_VTABLE)
            .inner(<alloc::string::String as Facet>::SHAPE)
            .eq()
            .build()
    };
}
