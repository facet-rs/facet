#![cfg(feature = "uuid")]

use alloc::{
    format,
    string::{String, ToString},
};
use uuid::Uuid;

use crate::{
    Def, Facet, OxPtrConst, OxPtrMut, ParseError, PtrConst, Shape, ShapeBuilder, Type, UserType,
    VTableIndirect,
};

unsafe fn display_uuid(
    source: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    unsafe {
        let uuid = source.get::<Uuid>();
        Some(write!(f, "{uuid}"))
    }
}

unsafe fn try_from_uuid(
    target: OxPtrMut,
    src_shape: &'static Shape,
    src: PtrConst,
) -> Option<Result<(), String>> {
    unsafe {
        if src_shape.id == <String as Facet>::SHAPE.id {
            let source_str = src.read::<String>();
            let parsed =
                Uuid::parse_str(&source_str).map_err(|_| "UUID parsing failed".to_string());
            Some(match parsed {
                Ok(val) => {
                    *target.as_mut::<Uuid>() = val;
                    Ok(())
                }
                Err(e) => Err(e),
            })
        } else {
            Some(Err(format!(
                "unsupported source shape for Uuid, expected String, got {}",
                src_shape.type_identifier
            )))
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

const UUID_VTABLE: VTableIndirect = VTableIndirect {
    display: Some(display_uuid),
    try_from: Some(try_from_uuid),
    parse: Some(parse_uuid),
    ..VTableIndirect::EMPTY
};

unsafe impl Facet<'_> for Uuid {
    const SHAPE: &'static Shape = &const {
        ShapeBuilder::for_sized::<Uuid>("Uuid")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_indirect(&UUID_VTABLE)
            .build()
    };
}
