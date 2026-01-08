#![cfg(feature = "uuid")]

use alloc::string::String;
use uuid::Uuid;

use crate::{
    Def, Facet, OxPtrConst, OxPtrMut, ParseError, PtrConst, Shape, ShapeBuilder, TryFromOutcome,
    Type, UserType, VTableIndirect,
};

unsafe fn try_from_uuid(
    target: OxPtrMut,
    src_shape: &'static Shape,
    src: PtrConst,
) -> TryFromOutcome {
    unsafe {
        // Handle &str (Copy type, use get)
        if src_shape.id == <&str as Facet>::SHAPE.id {
            let source_str: &str = src.get::<&str>();
            match Uuid::parse_str(source_str) {
                Ok(val) => {
                    *target.as_mut::<Uuid>() = val;
                    TryFromOutcome::Converted
                }
                Err(_) => TryFromOutcome::Failed("UUID parsing failed".into()),
            }
        }
        // Handle String (consume via read)
        else if src_shape.id == <String as Facet>::SHAPE.id {
            let source_str = src.read::<String>();
            match Uuid::parse_str(&source_str) {
                Ok(val) => {
                    *target.as_mut::<Uuid>() = val;
                    TryFromOutcome::Converted
                }
                Err(_) => TryFromOutcome::Failed("UUID parsing failed".into()),
            }
        } else {
            TryFromOutcome::Unsupported
        }
    }
}

unsafe fn parse_uuid(s: &str, target: OxPtrMut) -> Option<Result<(), ParseError>> {
    unsafe {
        let parsed = Uuid::parse_str(s).map_err(|_| ParseError::from_str("UUID parsing failed"));
        Some(match parsed {
            Ok(val) => {
                *target.as_mut::<Uuid>() = val;
                Ok(())
            }
            Err(e) => Err(e),
        })
    }
}

unsafe fn display_uuid(
    source: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    unsafe {
        let uuid = source.get::<Uuid>();
        Some(write!(f, "{uuid}"))
    }
}

unsafe fn partial_eq_uuid(a: OxPtrConst, b: OxPtrConst) -> Option<bool> {
    unsafe {
        let a = a.get::<Uuid>();
        let b = b.get::<Uuid>();
        Some(a == b)
    }
}

unsafe fn parse_bytes_uuid(bytes: &[u8], target: OxPtrMut) -> Option<Result<(), ParseError>> {
    unsafe {
        if bytes.len() != 16 {
            return Some(Err(ParseError::from_str("UUID must be exactly 16 bytes")));
        }
        let uuid = Uuid::from_slice(bytes).map_err(|_| ParseError::from_str("invalid UUID bytes"));
        Some(match uuid {
            Ok(val) => {
                *target.as_mut::<Uuid>() = val;
                Ok(())
            }
            Err(e) => Err(e),
        })
    }
}

const UUID_VTABLE: VTableIndirect = VTableIndirect {
    display: Some(display_uuid),
    try_from: Some(try_from_uuid),
    parse: Some(parse_uuid),
    parse_bytes: Some(parse_bytes_uuid),
    partial_eq: Some(partial_eq_uuid),
    ..VTableIndirect::EMPTY
};

unsafe impl Facet<'_> for Uuid {
    const SHAPE: &'static Shape = &const {
        ShapeBuilder::for_sized::<Uuid>("Uuid")
            .module_path("uuid")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_indirect(&UUID_VTABLE)
            .eq()
            .build()
    };
}
