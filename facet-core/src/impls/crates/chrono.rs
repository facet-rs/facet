#![cfg(feature = "chrono")]

use alloc::string::{String, ToString};
use chrono::{DateTime, Duration, FixedOffset, Local, NaiveDate, NaiveDateTime, NaiveTime, Utc};

use crate::{
    Def, Facet, OxPtrConst, OxPtrUninit, ParseError, ProxyDef, PtrConst, PtrMut, PtrUninit, Shape,
    ShapeBuilder, TryFromOutcome, Type, UserType, VTableIndirect,
};

// DateTime<Utc> implementation

unsafe fn display_datetime_utc(
    source: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    unsafe {
        let dt = source.get::<DateTime<Utc>>();
        use chrono::SecondsFormat;
        let s = dt.to_rfc3339_opts(SecondsFormat::Secs, true);
        Some(write!(f, "{s}"))
    }
}

unsafe fn try_from_datetime_utc(
    target: OxPtrUninit,
    src_shape: &'static Shape,
    src: PtrConst,
) -> TryFromOutcome {
    unsafe {
        if src_shape.id == <String as Facet>::SHAPE.id {
            let source_str = src.read::<String>();
            match DateTime::parse_from_rfc3339(&source_str).map(|dt| dt.with_timezone(&Utc)) {
                Ok(val) => {
                    target.put(val);
                    TryFromOutcome::Converted
                }
                Err(_) => TryFromOutcome::Failed("could not parse date".into()),
            }
        } else {
            TryFromOutcome::Unsupported
        }
    }
}

unsafe fn parse_datetime_utc(s: &str, target: OxPtrUninit) -> Option<Result<(), ParseError>> {
    unsafe {
        let parsed = DateTime::parse_from_rfc3339(s)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|_| ParseError::from_str("could not parse date"));
        Some(match parsed {
            Ok(val) => {
                target.put(val);
                Ok(())
            }
            Err(e) => Err(e),
        })
    }
}

unsafe fn partial_eq_datetime_utc(a: OxPtrConst, b: OxPtrConst) -> Option<bool> {
    unsafe {
        let a = a.get::<DateTime<Utc>>();
        let b = b.get::<DateTime<Utc>>();
        Some(a == b)
    }
}

const DATETIME_UTC_VTABLE: VTableIndirect = VTableIndirect {
    display: Some(display_datetime_utc),
    try_from: Some(try_from_datetime_utc),
    parse: Some(parse_datetime_utc),
    partial_eq: Some(partial_eq_datetime_utc),
    ..VTableIndirect::EMPTY
};

unsafe impl Facet<'_> for DateTime<Utc> {
    const SHAPE: &'static Shape = &const {
        ShapeBuilder::for_sized::<DateTime<Utc>>("DateTime<Utc>")
            .module_path("chrono")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_indirect(&DATETIME_UTC_VTABLE)
            .build()
    };
}

// DateTime<FixedOffset> implementation

unsafe fn display_datetime_fixed_offset(
    source: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    unsafe {
        let dt = source.get::<DateTime<FixedOffset>>();
        use chrono::SecondsFormat;
        Some(write!(
            f,
            "{}",
            dt.to_rfc3339_opts(SecondsFormat::Secs, true)
        ))
    }
}

unsafe fn try_from_datetime_fixed_offset(
    target: OxPtrUninit,
    src_shape: &'static Shape,
    src: PtrConst,
) -> TryFromOutcome {
    unsafe {
        if src_shape.id == <String as Facet>::SHAPE.id {
            let source_str = src.read::<String>();
            match DateTime::parse_from_rfc3339(&source_str) {
                Ok(val) => {
                    target.put(val);
                    TryFromOutcome::Converted
                }
                Err(_) => TryFromOutcome::Failed("could not parse date".into()),
            }
        } else {
            TryFromOutcome::Unsupported
        }
    }
}

unsafe fn parse_datetime_fixed_offset(
    s: &str,
    target: OxPtrUninit,
) -> Option<Result<(), ParseError>> {
    unsafe {
        let parsed = DateTime::parse_from_rfc3339(s)
            .map_err(|_| ParseError::from_str("could not parse date"));
        Some(match parsed {
            Ok(val) => {
                target.put(val);
                Ok(())
            }
            Err(e) => Err(e),
        })
    }
}

unsafe fn partial_eq_datetime_fixed_offset(a: OxPtrConst, b: OxPtrConst) -> Option<bool> {
    unsafe {
        let a = a.get::<DateTime<FixedOffset>>();
        let b = b.get::<DateTime<FixedOffset>>();
        Some(a == b)
    }
}

const DATETIME_FIXED_OFFSET_VTABLE: VTableIndirect = VTableIndirect {
    display: Some(display_datetime_fixed_offset),
    try_from: Some(try_from_datetime_fixed_offset),
    parse: Some(parse_datetime_fixed_offset),
    partial_eq: Some(partial_eq_datetime_fixed_offset),
    ..VTableIndirect::EMPTY
};

unsafe impl Facet<'_> for DateTime<FixedOffset> {
    const SHAPE: &'static Shape = &const {
        ShapeBuilder::for_sized::<DateTime<FixedOffset>>("DateTime<FixedOffset>")
            .module_path("chrono")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_indirect(&DATETIME_FIXED_OFFSET_VTABLE)
            .build()
    };
}

// DateTime<Local> implementation

unsafe fn display_datetime_local(
    source: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    unsafe {
        let dt = source.get::<DateTime<Local>>();
        use chrono::SecondsFormat;
        Some(write!(
            f,
            "{}",
            dt.to_rfc3339_opts(SecondsFormat::Secs, true)
        ))
    }
}

unsafe fn try_from_datetime_local(
    target: OxPtrUninit,
    src_shape: &'static Shape,
    src: PtrConst,
) -> TryFromOutcome {
    unsafe {
        if src_shape.id == <String as Facet>::SHAPE.id {
            let source_str = src.read::<String>();
            match DateTime::parse_from_rfc3339(&source_str).map(|dt| dt.with_timezone(&Local)) {
                Ok(val) => {
                    target.put(val);
                    TryFromOutcome::Converted
                }
                Err(_) => TryFromOutcome::Failed("could not parse date".into()),
            }
        } else {
            TryFromOutcome::Unsupported
        }
    }
}

unsafe fn parse_datetime_local(s: &str, target: OxPtrUninit) -> Option<Result<(), ParseError>> {
    unsafe {
        let parsed = DateTime::parse_from_rfc3339(s)
            .map(|dt| dt.with_timezone(&Local))
            .map_err(|_| ParseError::from_str("could not parse date"));
        Some(match parsed {
            Ok(val) => {
                target.put(val);
                Ok(())
            }
            Err(e) => Err(e),
        })
    }
}

unsafe fn partial_eq_datetime_local(a: OxPtrConst, b: OxPtrConst) -> Option<bool> {
    unsafe {
        let a = a.get::<DateTime<Local>>();
        let b = b.get::<DateTime<Local>>();
        Some(a == b)
    }
}

const DATETIME_LOCAL_VTABLE: VTableIndirect = VTableIndirect {
    display: Some(display_datetime_local),
    try_from: Some(try_from_datetime_local),
    parse: Some(parse_datetime_local),
    partial_eq: Some(partial_eq_datetime_local),
    ..VTableIndirect::EMPTY
};

unsafe impl Facet<'_> for DateTime<Local> {
    const SHAPE: &'static Shape = &const {
        ShapeBuilder::for_sized::<DateTime<Local>>("DateTime<Local>")
            .module_path("chrono")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_indirect(&DATETIME_LOCAL_VTABLE)
            .build()
    };
}

// NaiveDateTime implementation

unsafe fn display_naive_datetime(
    source: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    unsafe {
        let dt = source.get::<NaiveDateTime>();
        let formatted = dt.format("%Y-%m-%dT%H:%M:%S").to_string();
        Some(write!(f, "{formatted}"))
    }
}

unsafe fn try_from_naive_datetime(
    target: OxPtrUninit,
    src_shape: &'static Shape,
    src: PtrConst,
) -> TryFromOutcome {
    unsafe {
        if src_shape.id == <String as Facet>::SHAPE.id {
            let source_str = src.read::<String>();
            match NaiveDateTime::parse_from_str(&source_str, "%Y-%m-%dT%H:%M:%S")
                .or_else(|_| NaiveDateTime::parse_from_str(&source_str, "%Y-%m-%d %H:%M:%S"))
            {
                Ok(val) => {
                    target.put(val);
                    TryFromOutcome::Converted
                }
                Err(_) => TryFromOutcome::Failed("could not parse date".into()),
            }
        } else {
            TryFromOutcome::Unsupported
        }
    }
}

unsafe fn parse_naive_datetime(s: &str, target: OxPtrUninit) -> Option<Result<(), ParseError>> {
    unsafe {
        let parsed = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S")
            .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S"))
            .map_err(|_| ParseError::from_str("could not parse date"));
        Some(match parsed {
            Ok(val) => {
                target.put(val);
                Ok(())
            }
            Err(e) => Err(e),
        })
    }
}

unsafe fn partial_eq_naive_datetime(a: OxPtrConst, b: OxPtrConst) -> Option<bool> {
    unsafe {
        let a = a.get::<NaiveDateTime>();
        let b = b.get::<NaiveDateTime>();
        Some(a == b)
    }
}

const NAIVE_DATETIME_VTABLE: VTableIndirect = VTableIndirect {
    display: Some(display_naive_datetime),
    try_from: Some(try_from_naive_datetime),
    parse: Some(parse_naive_datetime),
    partial_eq: Some(partial_eq_naive_datetime),
    ..VTableIndirect::EMPTY
};

unsafe impl Facet<'_> for NaiveDateTime {
    const SHAPE: &'static Shape = &const {
        ShapeBuilder::for_sized::<NaiveDateTime>("NaiveDateTime")
            .module_path("chrono")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_indirect(&NAIVE_DATETIME_VTABLE)
            .build()
    };
}

// NaiveDate implementation

unsafe fn display_naive_date(
    source: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    unsafe {
        let dt = source.get::<NaiveDate>();
        let formatted = dt.format("%Y-%m-%d").to_string();
        Some(write!(f, "{formatted}"))
    }
}

unsafe fn try_from_naive_date(
    target: OxPtrUninit,
    src_shape: &'static Shape,
    src: PtrConst,
) -> TryFromOutcome {
    unsafe {
        if src_shape.id == <String as Facet>::SHAPE.id {
            let source_str = src.read::<String>();
            match NaiveDate::parse_from_str(&source_str, "%Y-%m-%d") {
                Ok(val) => {
                    target.put(val);
                    TryFromOutcome::Converted
                }
                Err(_) => TryFromOutcome::Failed("could not parse date".into()),
            }
        } else {
            TryFromOutcome::Unsupported
        }
    }
}

unsafe fn parse_naive_date(s: &str, target: OxPtrUninit) -> Option<Result<(), ParseError>> {
    unsafe {
        let parsed = NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .map_err(|_| ParseError::from_str("could not parse date"));
        Some(match parsed {
            Ok(val) => {
                target.put(val);
                Ok(())
            }
            Err(e) => Err(e),
        })
    }
}

unsafe fn partial_eq_naive_date(a: OxPtrConst, b: OxPtrConst) -> Option<bool> {
    unsafe {
        let a = a.get::<NaiveDate>();
        let b = b.get::<NaiveDate>();
        Some(a == b)
    }
}

const NAIVE_DATE_VTABLE: VTableIndirect = VTableIndirect {
    display: Some(display_naive_date),
    try_from: Some(try_from_naive_date),
    parse: Some(parse_naive_date),
    partial_eq: Some(partial_eq_naive_date),
    ..VTableIndirect::EMPTY
};

unsafe impl Facet<'_> for NaiveDate {
    const SHAPE: &'static Shape = &const {
        ShapeBuilder::for_sized::<NaiveDate>("NaiveDate")
            .module_path("chrono")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_indirect(&NAIVE_DATE_VTABLE)
            .build()
    };
}

// NaiveTime implementation

unsafe fn display_naive_time(
    source: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    unsafe {
        let dt = source.get::<NaiveTime>();
        let formatted = dt.format("%H:%M:%S").to_string();
        Some(write!(f, "{formatted}"))
    }
}

unsafe fn try_from_naive_time(
    target: OxPtrUninit,
    src_shape: &'static Shape,
    src: PtrConst,
) -> TryFromOutcome {
    unsafe {
        if src_shape.id == <String as Facet>::SHAPE.id {
            let source_str = src.read::<String>();
            match NaiveTime::parse_from_str(&source_str, "%H:%M:%S")
                .or_else(|_| NaiveTime::parse_from_str(&source_str, "%H:%M:%S%.f"))
            {
                Ok(val) => {
                    target.put(val);
                    TryFromOutcome::Converted
                }
                Err(_) => TryFromOutcome::Failed("could not parse time".into()),
            }
        } else {
            TryFromOutcome::Unsupported
        }
    }
}

unsafe fn parse_naive_time(s: &str, target: OxPtrUninit) -> Option<Result<(), ParseError>> {
    unsafe {
        let parsed = NaiveTime::parse_from_str(s, "%H:%M:%S")
            .or_else(|_| NaiveTime::parse_from_str(s, "%H:%M:%S%.f"))
            .map_err(|_| ParseError::from_str("could not parse time"));
        Some(match parsed {
            Ok(val) => {
                target.put(val);
                Ok(())
            }
            Err(e) => Err(e),
        })
    }
}

unsafe fn partial_eq_naive_time(a: OxPtrConst, b: OxPtrConst) -> Option<bool> {
    unsafe {
        let a = a.get::<NaiveTime>();
        let b = b.get::<NaiveTime>();
        Some(a == b)
    }
}

const NAIVE_TIME_VTABLE: VTableIndirect = VTableIndirect {
    display: Some(display_naive_time),
    try_from: Some(try_from_naive_time),
    parse: Some(parse_naive_time),
    partial_eq: Some(partial_eq_naive_time),
    ..VTableIndirect::EMPTY
};

unsafe impl Facet<'_> for NaiveTime {
    const SHAPE: &'static Shape = &const {
        ShapeBuilder::for_sized::<NaiveTime>("NaiveTime")
            .module_path("chrono")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_indirect(&NAIVE_TIME_VTABLE)
            .build()
    };
}

// Duration implementation

unsafe fn duration_proxy_convert_out(
    target_ptr: PtrConst,
    proxy_ptr: PtrUninit,
) -> Result<PtrMut, String> {
    unsafe {
        let duration = target_ptr.get::<Duration>();
        let secs = duration.num_seconds();
        let nanos = duration.subsec_nanos();
        let proxy_mut = proxy_ptr.as_mut_byte_ptr() as *mut (i64, i32);
        proxy_mut.write((secs, nanos));
        Ok(PtrMut::new(proxy_mut as *mut u8))
    }
}

unsafe fn duration_proxy_convert_in(
    proxy_ptr: PtrConst,
    target_ptr: PtrUninit,
) -> Result<PtrMut, String> {
    unsafe {
        let (secs, nanos): (i64, i32) = proxy_ptr.read::<(i64, i32)>();
        let duration = match Duration::try_seconds(secs) {
            Some(d) => d + Duration::nanoseconds(nanos as i64),
            None => return Err("Duration seconds overflow".into()),
        };
        let target_mut = target_ptr.as_mut_byte_ptr() as *mut Duration;
        target_mut.write(duration);
        Ok(PtrMut::new(target_mut as *mut u8))
    }
}

const DURATION_PROXY: ProxyDef = ProxyDef {
    shape: <(i64, i32) as Facet>::SHAPE,
    convert_in: duration_proxy_convert_in,
    convert_out: duration_proxy_convert_out,
};

unsafe fn display_duration(
    source: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    unsafe {
        let d = source.get::<Duration>();
        Some(write!(f, "{}s {}ns", d.num_seconds(), d.subsec_nanos()))
    }
}

unsafe fn partial_eq_duration(a: OxPtrConst, b: OxPtrConst) -> Option<bool> {
    unsafe { Some(a.get::<Duration>() == b.get::<Duration>()) }
}

const DURATION_VTABLE: VTableIndirect = VTableIndirect {
    display: Some(display_duration),
    partial_eq: Some(partial_eq_duration),
    ..VTableIndirect::EMPTY
};

unsafe impl Facet<'_> for Duration {
    const SHAPE: &'static Shape = &const {
        ShapeBuilder::for_sized::<Duration>("Duration")
            .module_path("chrono")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_indirect(&DURATION_VTABLE)
            .proxy(&DURATION_PROXY)
            .build()
    };
}
