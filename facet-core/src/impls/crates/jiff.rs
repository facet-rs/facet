#![cfg(feature = "jiff02")]

use alloc::string::String;
use jiff::{Timestamp, Zoned, civil::DateTime};

use crate::{
    Def, Facet, OxPtrConst, OxPtrMut, ParseError, PtrConst, Shape, ShapeBuilder, TryFromOutcome,
    Type, UserType, VTableIndirect,
};

// ============================================================================
// Zoned
// ============================================================================

const ZONED_ERROR: &str = "could not parse time-zone aware instant of time";

/// Display for Zoned
unsafe fn zoned_display(
    source: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    unsafe {
        let zoned = source.get::<Zoned>();
        Some(write!(f, "{zoned}"))
    }
}

/// Parse for Zoned
unsafe fn zoned_parse(s: &str, target: OxPtrMut) -> Option<Result<(), ParseError>> {
    let parsed = s
        .parse::<Zoned>()
        .map_err(|_| ParseError::from_str(ZONED_ERROR));
    Some(match parsed {
        Ok(val) => unsafe {
            let dst_ptr: *mut Zoned = core::mem::transmute(target.ptr().as_mut_byte_ptr());
            dst_ptr.write(val);
            Ok(())
        },
        Err(e) => Err(e),
    })
}

/// TryFrom for Zoned (from String)
unsafe fn zoned_try_from(
    target: OxPtrMut,
    src_shape: &'static Shape,
    src: PtrConst,
) -> TryFromOutcome {
    if src_shape.id == <String as Facet>::SHAPE.id {
        unsafe {
            let source_str = src.read::<String>();
            match source_str.parse::<Zoned>() {
                Ok(val) => {
                    let dst_ptr: *mut Zoned = core::mem::transmute(target.ptr().as_mut_byte_ptr());
                    dst_ptr.write(val);
                    TryFromOutcome::Converted
                }
                Err(_) => TryFromOutcome::Failed(ZONED_ERROR.into()),
            }
        }
    } else {
        TryFromOutcome::Unsupported
    }
}

unsafe fn zoned_partial_eq(a: OxPtrConst, b: OxPtrConst) -> Option<bool> {
    unsafe {
        let a = a.get::<Zoned>();
        let b = b.get::<Zoned>();
        Some(a == b)
    }
}

unsafe impl Facet<'_> for Zoned {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableIndirect = VTableIndirect {
            display: Some(zoned_display),
            parse: Some(zoned_parse),
            try_from: Some(zoned_try_from),
            partial_eq: Some(zoned_partial_eq),
            ..VTableIndirect::EMPTY
        };

        ShapeBuilder::for_sized::<Zoned>("Zoned")
            .module_path("jiff")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_indirect(&VTABLE)
            .build()
    };
}

// ============================================================================
// Timestamp
// ============================================================================

const TIMESTAMP_ERROR: &str = "could not parse timestamp";

/// Display for Timestamp
unsafe fn timestamp_display(
    source: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    unsafe {
        let timestamp = source.get::<Timestamp>();
        Some(write!(f, "{timestamp}"))
    }
}

/// Parse for Timestamp
unsafe fn timestamp_parse(s: &str, target: OxPtrMut) -> Option<Result<(), ParseError>> {
    let parsed = s
        .parse::<Timestamp>()
        .map_err(|_| ParseError::from_str(TIMESTAMP_ERROR));
    Some(match parsed {
        Ok(val) => unsafe {
            let dst_ptr: *mut Timestamp = core::mem::transmute(target.ptr().as_mut_byte_ptr());
            dst_ptr.write(val);
            Ok(())
        },
        Err(e) => Err(e),
    })
}

/// TryFrom for Timestamp (from String)
unsafe fn timestamp_try_from(
    target: OxPtrMut,
    src_shape: &'static Shape,
    src: PtrConst,
) -> TryFromOutcome {
    if src_shape.id == <String as Facet>::SHAPE.id {
        unsafe {
            let source_str = src.read::<String>();
            match source_str.parse::<Timestamp>() {
                Ok(val) => {
                    let dst_ptr: *mut Timestamp =
                        core::mem::transmute(target.ptr().as_mut_byte_ptr());
                    dst_ptr.write(val);
                    TryFromOutcome::Converted
                }
                Err(_) => TryFromOutcome::Failed(TIMESTAMP_ERROR.into()),
            }
        }
    } else {
        TryFromOutcome::Unsupported
    }
}

unsafe fn timestamp_partial_eq(a: OxPtrConst, b: OxPtrConst) -> Option<bool> {
    unsafe {
        let a = a.get::<Timestamp>();
        let b = b.get::<Timestamp>();
        Some(a == b)
    }
}

unsafe impl Facet<'_> for Timestamp {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableIndirect = VTableIndirect {
            display: Some(timestamp_display),
            parse: Some(timestamp_parse),
            try_from: Some(timestamp_try_from),
            partial_eq: Some(timestamp_partial_eq),
            ..VTableIndirect::EMPTY
        };

        ShapeBuilder::for_sized::<Timestamp>("Timestamp")
            .module_path("jiff")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_indirect(&VTABLE)
            .build()
    };
}

// ============================================================================
// DateTime
// ============================================================================

const DATETIME_ERROR: &str = "could not parse civil datetime";

/// Display for DateTime
unsafe fn datetime_display(
    source: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    unsafe {
        let datetime = source.get::<DateTime>();
        Some(write!(f, "{datetime}"))
    }
}

/// Parse for DateTime
unsafe fn datetime_parse(s: &str, target: OxPtrMut) -> Option<Result<(), ParseError>> {
    let parsed = s
        .parse::<DateTime>()
        .map_err(|_| ParseError::from_str(DATETIME_ERROR));
    Some(match parsed {
        Ok(val) => unsafe {
            let dst_ptr: *mut DateTime = core::mem::transmute(target.ptr().as_mut_byte_ptr());
            dst_ptr.write(val);
            Ok(())
        },
        Err(e) => Err(e),
    })
}

/// TryFrom for DateTime (from String)
unsafe fn datetime_try_from(
    target: OxPtrMut,
    src_shape: &'static Shape,
    src: PtrConst,
) -> TryFromOutcome {
    if src_shape.id == <String as Facet>::SHAPE.id {
        unsafe {
            let source_str = src.read::<String>();
            match source_str.parse::<DateTime>() {
                Ok(val) => {
                    let dst_ptr: *mut DateTime =
                        core::mem::transmute(target.ptr().as_mut_byte_ptr());
                    dst_ptr.write(val);
                    TryFromOutcome::Converted
                }
                Err(_) => TryFromOutcome::Failed(DATETIME_ERROR.into()),
            }
        }
    } else {
        TryFromOutcome::Unsupported
    }
}

unsafe fn datetime_partial_eq(a: OxPtrConst, b: OxPtrConst) -> Option<bool> {
    unsafe {
        let a = a.get::<DateTime>();
        let b = b.get::<DateTime>();
        Some(a == b)
    }
}

unsafe impl Facet<'_> for DateTime {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableIndirect = VTableIndirect {
            display: Some(datetime_display),
            parse: Some(datetime_parse),
            try_from: Some(datetime_try_from),
            partial_eq: Some(datetime_partial_eq),
            ..VTableIndirect::EMPTY
        };

        ShapeBuilder::for_sized::<DateTime>("DateTime")
            .module_path("jiff")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_indirect(&VTABLE)
            .build()
    };
}

#[cfg(test)]
mod tests {
    use core::fmt;

    use jiff::{Timestamp, civil::DateTime};

    use crate::{Facet, PtrConst, PtrMut};

    #[test]
    #[cfg(not(miri))] // I don't think we can read time zones from miri, the test just fails
    fn parse_zoned() {
        use jiff::Zoned;

        facet_testhelpers::setup();

        let target = Zoned::SHAPE.allocate().unwrap();
        let target_mut = PtrMut::new(target.as_mut_byte_ptr());
        unsafe {
            Zoned::SHAPE
                .call_parse("2023-12-31T18:30:00+07:00[Asia/Ho_Chi_Minh]", target_mut)
                .unwrap()
                .unwrap();
        }
        let odt: Zoned = unsafe { target.assume_init().read() };
        assert_eq!(
            odt,
            "2023-12-31T18:30:00+07:00[Asia/Ho_Chi_Minh]"
                .parse()
                .unwrap()
        );

        {
            struct DisplayWrapper(PtrConst);

            impl fmt::Display for DisplayWrapper {
                fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    unsafe { Zoned::SHAPE.call_display(self.0, f).unwrap() }
                }
            }

            let s = format!("{}", DisplayWrapper(PtrConst::new(&odt as *const Zoned)));
            assert_eq!(s, "2023-12-31T18:30:00+07:00[Asia/Ho_Chi_Minh]");
        }

        // Deallocate the heap allocation to avoid memory leaks under Miri
        unsafe {
            Zoned::SHAPE.deallocate_uninit(target).unwrap();
        }
    }

    #[test]
    fn parse_timestamp() {
        facet_testhelpers::setup();

        let target = Timestamp::SHAPE.allocate().unwrap();
        let target_mut = PtrMut::new(target.as_mut_byte_ptr());
        unsafe {
            Timestamp::SHAPE
                .call_parse("2024-06-19T15:22:45Z", target_mut)
                .unwrap()
                .unwrap();
        }
        let odt: Timestamp = unsafe { target.assume_init().read() };
        assert_eq!(odt, "2024-06-19T15:22:45Z".parse().unwrap());

        {
            struct DisplayWrapper(PtrConst);

            impl fmt::Display for DisplayWrapper {
                fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    unsafe { Timestamp::SHAPE.call_display(self.0, f).unwrap() }
                }
            }

            let s = format!(
                "{}",
                DisplayWrapper(PtrConst::new(&odt as *const Timestamp))
            );
            assert_eq!(s, "2024-06-19T15:22:45Z");
        }

        // Deallocate the heap allocation to avoid memory leaks under Miri
        unsafe {
            Timestamp::SHAPE.deallocate_uninit(target).unwrap();
        }
    }

    #[test]
    fn parse_datetime() {
        facet_testhelpers::setup();

        let target = DateTime::SHAPE.allocate().unwrap();
        let target_mut = PtrMut::new(target.as_mut_byte_ptr());
        unsafe {
            DateTime::SHAPE
                .call_parse("2024-06-19T15:22:45", target_mut)
                .unwrap()
                .unwrap();
        }
        let odt: DateTime = unsafe { target.assume_init().read() };
        assert_eq!(odt, "2024-06-19T15:22:45".parse().unwrap());

        {
            struct DisplayWrapper(PtrConst);

            impl fmt::Display for DisplayWrapper {
                fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    unsafe { DateTime::SHAPE.call_display(self.0, f).unwrap() }
                }
            }

            let s = format!("{}", DisplayWrapper(PtrConst::new(&odt as *const DateTime)));
            assert_eq!(s, "2024-06-19T15:22:45");
        }

        // Deallocate the heap allocation to avoid memory leaks under Miri
        unsafe {
            DateTime::SHAPE.deallocate_uninit(target).unwrap();
        }
    }
}
