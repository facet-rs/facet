#![cfg(feature = "chrono")]

use alloc::string::{String, ToString};
use chrono::{DateTime, FixedOffset, Local, NaiveDate, NaiveDateTime, NaiveTime, Utc};

use crate::{
    Def, Facet, OxPtrConst, OxPtrMut, ParseError, PtrConst, Shape, ShapeBuilder, TryFromOutcome,
    Type, UserType, VTableIndirect,
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
    target: OxPtrMut,
    src_shape: &'static Shape,
    src: PtrConst,
) -> TryFromOutcome {
    unsafe {
        if src_shape.id == <String as Facet>::SHAPE.id {
            let source_str = src.read::<String>();
            match DateTime::parse_from_rfc3339(&source_str).map(|dt| dt.with_timezone(&Utc)) {
                Ok(val) => {
                    *target.as_mut::<DateTime<Utc>>() = val;
                    TryFromOutcome::Converted
                }
                Err(_) => TryFromOutcome::Failed("could not parse date".into()),
            }
        } else {
            TryFromOutcome::Unsupported
        }
    }
}

unsafe fn parse_datetime_utc(s: &str, target: OxPtrMut) -> Option<Result<(), ParseError>> {
    unsafe {
        let parsed = DateTime::parse_from_rfc3339(s)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|_| ParseError::from_str("could not parse date"));
        Some(match parsed {
            Ok(val) => {
                *target.as_mut::<DateTime<Utc>>() = val;
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
            .decl_id_prim()
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
    target: OxPtrMut,
    src_shape: &'static Shape,
    src: PtrConst,
) -> TryFromOutcome {
    unsafe {
        if src_shape.id == <String as Facet>::SHAPE.id {
            let source_str = src.read::<String>();
            match DateTime::parse_from_rfc3339(&source_str) {
                Ok(val) => {
                    *target.as_mut::<DateTime<FixedOffset>>() = val;
                    TryFromOutcome::Converted
                }
                Err(_) => TryFromOutcome::Failed("could not parse date".into()),
            }
        } else {
            TryFromOutcome::Unsupported
        }
    }
}

unsafe fn parse_datetime_fixed_offset(s: &str, target: OxPtrMut) -> Option<Result<(), ParseError>> {
    unsafe {
        let parsed = DateTime::parse_from_rfc3339(s)
            .map_err(|_| ParseError::from_str("could not parse date"));
        Some(match parsed {
            Ok(val) => {
                *target.as_mut::<DateTime<FixedOffset>>() = val;
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
            .decl_id_prim()
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
    target: OxPtrMut,
    src_shape: &'static Shape,
    src: PtrConst,
) -> TryFromOutcome {
    unsafe {
        if src_shape.id == <String as Facet>::SHAPE.id {
            let source_str = src.read::<String>();
            match DateTime::parse_from_rfc3339(&source_str).map(|dt| dt.with_timezone(&Local)) {
                Ok(val) => {
                    *target.as_mut::<DateTime<Local>>() = val;
                    TryFromOutcome::Converted
                }
                Err(_) => TryFromOutcome::Failed("could not parse date".into()),
            }
        } else {
            TryFromOutcome::Unsupported
        }
    }
}

unsafe fn parse_datetime_local(s: &str, target: OxPtrMut) -> Option<Result<(), ParseError>> {
    unsafe {
        let parsed = DateTime::parse_from_rfc3339(s)
            .map(|dt| dt.with_timezone(&Local))
            .map_err(|_| ParseError::from_str("could not parse date"));
        Some(match parsed {
            Ok(val) => {
                *target.as_mut::<DateTime<Local>>() = val;
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
            .decl_id_prim()
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
    target: OxPtrMut,
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
                    *target.as_mut::<NaiveDateTime>() = val;
                    TryFromOutcome::Converted
                }
                Err(_) => TryFromOutcome::Failed("could not parse date".into()),
            }
        } else {
            TryFromOutcome::Unsupported
        }
    }
}

unsafe fn parse_naive_datetime(s: &str, target: OxPtrMut) -> Option<Result<(), ParseError>> {
    unsafe {
        let parsed = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S")
            .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S"))
            .map_err(|_| ParseError::from_str("could not parse date"));
        Some(match parsed {
            Ok(val) => {
                *target.as_mut::<NaiveDateTime>() = val;
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
            .decl_id_prim()
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
    target: OxPtrMut,
    src_shape: &'static Shape,
    src: PtrConst,
) -> TryFromOutcome {
    unsafe {
        if src_shape.id == <String as Facet>::SHAPE.id {
            let source_str = src.read::<String>();
            match NaiveDate::parse_from_str(&source_str, "%Y-%m-%d") {
                Ok(val) => {
                    *target.as_mut::<NaiveDate>() = val;
                    TryFromOutcome::Converted
                }
                Err(_) => TryFromOutcome::Failed("could not parse date".into()),
            }
        } else {
            TryFromOutcome::Unsupported
        }
    }
}

unsafe fn parse_naive_date(s: &str, target: OxPtrMut) -> Option<Result<(), ParseError>> {
    unsafe {
        let parsed = NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .map_err(|_| ParseError::from_str("could not parse date"));
        Some(match parsed {
            Ok(val) => {
                *target.as_mut::<NaiveDate>() = val;
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
            .decl_id_prim()
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
    target: OxPtrMut,
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
                    *target.as_mut::<NaiveTime>() = val;
                    TryFromOutcome::Converted
                }
                Err(_) => TryFromOutcome::Failed("could not parse time".into()),
            }
        } else {
            TryFromOutcome::Unsupported
        }
    }
}

unsafe fn parse_naive_time(s: &str, target: OxPtrMut) -> Option<Result<(), ParseError>> {
    unsafe {
        let parsed = NaiveTime::parse_from_str(s, "%H:%M:%S")
            .or_else(|_| NaiveTime::parse_from_str(s, "%H:%M:%S%.f"))
            .map_err(|_| ParseError::from_str("could not parse time"));
        Some(match parsed {
            Ok(val) => {
                *target.as_mut::<NaiveTime>() = val;
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
            .decl_id_prim()
            .module_path("chrono")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_indirect(&NAIVE_TIME_VTABLE)
            .build()
    };
}
