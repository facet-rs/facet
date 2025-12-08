#![cfg(feature = "ulid")]

use alloc::{
    format,
    string::{String, ToString},
};

use ulid::Ulid;

use crate::{
    Def, Facet, OxPtrConst, OxPtrMut, ParseError, Shape, ShapeBuilder, Type, UserType,
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

unsafe fn try_from_ulid(source: OxPtrConst, target: OxPtrMut) -> Option<Result<(), String>> {
    unsafe {
        if source.shape.is_type::<String>() {
            let source_str = source.ptr().read::<String>();
            let parsed =
                Ulid::from_string(&source_str).map_err(|_| "ULID parsing failed".to_string());
            Some(match parsed {
                Ok(val) => {
                    *target.as_mut::<Ulid>() = val;
                    Ok(())
                }
                Err(e) => Err(e),
            })
        } else {
            Some(Err(format!(
                "unsupported source shape for Ulid, expected String, got {}",
                source.shape.type_identifier
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
