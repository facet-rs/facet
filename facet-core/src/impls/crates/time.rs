#![cfg(feature = "time")]

use alloc::{
    format,
    string::{String, ToString},
};
use time::{OffsetDateTime, UtcDateTime};

use crate::{
    Def, Facet, OxPtrConst, OxPtrMut, ParseError, Shape, ShapeBuilder, Type, UserType,
    VTableIndirect,
};

// UtcDateTime implementation

unsafe fn utc_display(
    source: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    unsafe {
        let udt = source.get::<UtcDateTime>();
        Some(
            match udt.format(&time::format_description::well_known::Rfc3339) {
                Ok(s) => write!(f, "{s}"),
                Err(_) => write!(f, "<invalid UtcDateTime>"),
            },
        )
    }
}

unsafe fn utc_try_from(source: OxPtrConst, target: OxPtrMut) -> Option<Result<(), String>> {
    unsafe {
        if source.shape.is_type::<String>() {
            let source_str = source.ptr().read::<String>();
            let parsed =
                UtcDateTime::parse(&source_str, &time::format_description::well_known::Rfc3339)
                    .map_err(|_| "could not parse date".to_string());
            Some(match parsed {
                Ok(val) => {
                    *target.as_mut::<UtcDateTime>() = val;
                    Ok(())
                }
                Err(e) => Err(e),
            })
        } else {
            Some(Err(format!(
                "unsupported source shape for UtcDateTime, expected String, got {}",
                source.shape.type_identifier
            )))
        }
    }
}

unsafe fn utc_parse(s: &str, target: OxPtrMut) -> Option<Result<(), ParseError>> {
    unsafe {
        let parsed = UtcDateTime::parse(s, &time::format_description::well_known::Rfc3339)
            .map_err(|_| ParseError::from_str("could not parse date"));
        Some(match parsed {
            Ok(val) => {
                *target.as_mut::<UtcDateTime>() = val;
                Ok(())
            }
            Err(e) => Err(e),
        })
    }
}

const UTC_VTABLE: VTableIndirect = VTableIndirect {
    display: Some(utc_display),
    try_from: Some(utc_try_from),
    parse: Some(utc_parse),
    ..VTableIndirect::EMPTY
};

unsafe impl Facet<'_> for UtcDateTime {
    const SHAPE: &'static Shape = &const {
        ShapeBuilder::for_sized::<UtcDateTime>("UtcDateTime")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_indirect(&UTC_VTABLE)
            .build()
    };
}

// OffsetDateTime implementation

unsafe fn offset_display(
    source: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    unsafe {
        let odt = source.get::<OffsetDateTime>();
        Some(
            match odt.format(&time::format_description::well_known::Rfc3339) {
                Ok(s) => write!(f, "{s}"),
                Err(_) => write!(f, "<invalid OffsetDateTime>"),
            },
        )
    }
}

unsafe fn offset_try_from(source: OxPtrConst, target: OxPtrMut) -> Option<Result<(), String>> {
    unsafe {
        if source.shape.is_type::<String>() {
            let source_str = source.ptr().read::<String>();
            let parsed =
                OffsetDateTime::parse(&source_str, &time::format_description::well_known::Rfc3339)
                    .map_err(|_| "could not parse date".to_string());
            Some(match parsed {
                Ok(val) => {
                    *target.as_mut::<OffsetDateTime>() = val;
                    Ok(())
                }
                Err(e) => Err(e),
            })
        } else {
            Some(Err(format!(
                "unsupported source shape for OffsetDateTime, expected String, got {}",
                source.shape.type_identifier
            )))
        }
    }
}

unsafe fn offset_parse(s: &str, target: OxPtrMut) -> Option<Result<(), ParseError>> {
    unsafe {
        let parsed = OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339)
            .map_err(|_| ParseError::from_str("could not parse date"));
        Some(match parsed {
            Ok(val) => {
                *target.as_mut::<OffsetDateTime>() = val;
                Ok(())
            }
            Err(e) => Err(e),
        })
    }
}

const OFFSET_VTABLE: VTableIndirect = VTableIndirect {
    display: Some(offset_display),
    try_from: Some(offset_try_from),
    parse: Some(offset_parse),
    ..VTableIndirect::EMPTY
};

unsafe impl Facet<'_> for OffsetDateTime {
    const SHAPE: &'static Shape = &const {
        ShapeBuilder::for_sized::<OffsetDateTime>("OffsetDateTime")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_indirect(&OFFSET_VTABLE)
            .build()
    };
}

#[cfg(test)]
mod tests {
    use core::fmt;

    use time::OffsetDateTime;

    use crate::{Facet, PtrConst};

    #[test]
    fn parse_offset_date_time() {
        facet_testhelpers::setup();

        let target = OffsetDateTime::SHAPE.allocate().unwrap();
        unsafe {
            // SAFETY: parse will initialize the memory, and PtrUninit/PtrMut have the same layout
            let target_mut = core::mem::transmute::<crate::PtrUninit, crate::PtrMut>(target);
            OffsetDateTime::SHAPE
                .call_parse("2023-03-14T15:09:26Z", target_mut)
                .unwrap()
                .unwrap();
        }
        let odt: OffsetDateTime = unsafe { target.assume_init().read() };
        assert_eq!(
            odt,
            OffsetDateTime::parse(
                "2023-03-14T15:09:26Z",
                &time::format_description::well_known::Rfc3339
            )
            .unwrap()
        );

        {
            struct DisplayWrapper(PtrConst);

            impl fmt::Display for DisplayWrapper {
                fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    unsafe { OffsetDateTime::SHAPE.call_display(self.0, f).unwrap() }
                }
            }

            let s = format!(
                "{}",
                DisplayWrapper(unsafe { target.assume_init().as_const() })
            );
            assert_eq!(s, "2023-03-14T15:09:26Z");
        }

        // Deallocate the heap allocation to avoid memory leaks under Miri
        unsafe {
            OffsetDateTime::SHAPE.deallocate_uninit(target).unwrap();
        }
    }
}
