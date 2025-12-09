#![cfg(feature = "ulid")]

use alloc::{
    format,
    string::{String, ToString},
};

use ulid::Ulid;

use crate::{
    Def, Facet, OxPtrConst, OxPtrMut, ParseError, PtrConst, Shape, ShapeBuilder, Type, UserType,
    VTableIndirect,
};

unsafe fn display_ulid(
    source: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    unsafe {
        let ulid = source.get::<Ulid>();
        Some(write!(f, "{ulid}"))
    }
}

unsafe fn try_from_ulid(
    target: OxPtrMut,
    src_shape: &'static Shape,
    src: PtrConst,
) -> Option<Result<(), String>> {
    unsafe {
        // Handle &str
        if src_shape.id == <&str as Facet>::SHAPE.id {
            let source_str: &str = src.get::<&str>();
            match Ulid::from_string(source_str) {
                Ok(val) => {
                    *target.as_mut::<Ulid>() = val;
                    Some(Ok(()))
                }
                Err(_) => Some(Err("ULID parsing failed".to_string())),
            }
        }
        // Handle String
        else if src_shape.id == <String as Facet>::SHAPE.id {
            let source_str = src.read::<String>();
            match Ulid::from_string(&source_str) {
                Ok(val) => {
                    *target.as_mut::<Ulid>() = val;
                    Some(Ok(()))
                }
                Err(_) => Some(Err("ULID parsing failed".to_string())),
            }
        } else {
            Some(Err(format!(
                "unsupported source shape for Ulid, expected &str or String, got {}",
                src_shape.type_identifier
            )))
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

const ULID_VTABLE: VTableIndirect = VTableIndirect {
    display: Some(display_ulid),
    try_from: Some(try_from_ulid),
    parse: Some(parse_ulid),
    ..VTableIndirect::EMPTY
};

unsafe impl Facet<'_> for Ulid {
    const SHAPE: &'static Shape = &const {
        ShapeBuilder::for_sized::<Ulid>("Ulid")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_indirect(&ULID_VTABLE)
            .inner(<String as Facet>::SHAPE)
            .build()
    };
}
