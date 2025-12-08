use alloc::string::{String, ToString};
use chrono::{DateTime, FixedOffset, Local, NaiveDate, NaiveDateTime, NaiveTime, Utc};

use crate::{
    Def, Facet, ParseError, PtrConst, PtrUninit, Shape, Type, UserType, Variance, value_vtable,
};

unsafe impl Facet<'_> for DateTime<Utc> {
    const SHAPE: &'static Shape = &const {
        Shape {
            id: Shape::id_of::<Self>(),
            layout: Shape::layout_of::<Self>(),
            vtable: {
                let mut vtable = value_vtable!(DateTime<Utc>, |f, _opts| write!(
                    f,
                    "{}",
                    Self::SHAPE.type_identifier
                ));
                {
                    vtable.try_from = {
                        Some(
                            |source: PtrConst, source_shape: &Shape, target: PtrUninit| {
                                if source_shape.is_type::<String>() {
                                    let source = unsafe { source.read::<String>() };
                                    let parsed = DateTime::parse_from_rfc3339(&source)
                                        .map(|dt| dt.with_timezone(&Utc))
                                        .map_err(|_| ParseError::Generic("could not parse date"));
                                    match parsed {
                                        Ok(val) => Ok(unsafe { target.put(val) }),
                                        Err(_e) => Err(crate::TryFromError::Generic(
                                            "could not parse date",
                                        )),
                                    }
                                } else {
                                    Err(crate::TryFromError::UnsupportedSourceShape {
                                        src_shape: source_shape,
                                        expected: &[String::SHAPE],
                                    })
                                }
                            },
                        )
                    };
                    vtable.parse = {
                        Some(|s: &str, target: PtrUninit| {
                            let parsed = DateTime::parse_from_rfc3339(s)
                                .map(|dt| dt.with_timezone(&Utc))
                                .map_err(|_| ParseError::Generic("could not parse date"))?;
                            Ok(unsafe { target.put(parsed) })
                        })
                    };
                    vtable.format.display = Some(|value, f| unsafe {
                        let dt = value.get::<DateTime<Utc>>();
                        use chrono::SecondsFormat;
                        let s = dt.to_rfc3339_opts(SecondsFormat::Secs, true);
                        write!(f, "{s}")
                    });
                }
                vtable
            },
            ty: Type::User(UserType::Opaque),
            def: Def::Scalar,
            type_identifier: "DateTime<Utc>",
            type_params: &[],
            doc: &[],
            attributes: &[],
            type_tag: None,
            inner: None,
            proxy: None,
            variance: Variance::Invariant,
        }
    };
}

unsafe impl Facet<'_> for DateTime<FixedOffset> {
    const SHAPE: &'static Shape = &const {
        Shape {
            id: Shape::id_of::<Self>(),
            layout: Shape::layout_of::<Self>(),
            vtable: {
                let mut vtable = value_vtable!(DateTime<FixedOffset>, |f, _opts| write!(
                    f,
                    "{}",
                    Self::SHAPE.type_identifier
                ));
                {
                    vtable.try_from = {
                        Some(
                            |source: PtrConst, source_shape: &Shape, target: PtrUninit| {
                                if source_shape.is_type::<String>() {
                                    let source = unsafe { source.read::<String>() };
                                    let parsed = DateTime::parse_from_rfc3339(&source)
                                        .map_err(|_| ParseError::Generic("could not parse date"));
                                    match parsed {
                                        Ok(val) => Ok(unsafe { target.put(val) }),
                                        Err(_e) => Err(crate::TryFromError::Generic(
                                            "could not parse date",
                                        )),
                                    }
                                } else {
                                    Err(crate::TryFromError::UnsupportedSourceShape {
                                        src_shape: source_shape,
                                        expected: &[String::SHAPE],
                                    })
                                }
                            },
                        )
                    };
                    vtable.parse = {
                        Some(|s: &str, target: PtrUninit| {
                            let parsed = DateTime::parse_from_rfc3339(s)
                                .map_err(|_| ParseError::Generic("could not parse date"))?;
                            Ok(unsafe { target.put(parsed) })
                        })
                    };
                    vtable.format.display = Some(|value, f| unsafe {
                        let dt = value.get::<DateTime<FixedOffset>>();
                        use chrono::SecondsFormat;
                        write!(f, "{}", dt.to_rfc3339_opts(SecondsFormat::Secs, true))
                    });
                }
                vtable
            },
            ty: Type::User(UserType::Opaque),
            def: Def::Scalar,
            type_identifier: "DateTime<FixedOffset>",
            type_params: &[],
            doc: &[],
            attributes: &[],
            type_tag: None,
            inner: None,
            proxy: None,
            variance: Variance::Invariant,
        }
    };
}

unsafe impl Facet<'_> for DateTime<Local> {
    const SHAPE: &'static Shape = &const {
        Shape {
            id: Shape::id_of::<Self>(),
            layout: Shape::layout_of::<Self>(),
            vtable: {
                let mut vtable = value_vtable!(DateTime<Local>, |f, _opts| write!(
                    f,
                    "{}",
                    Self::SHAPE.type_identifier
                ));
                {
                    vtable.try_from = {
                        Some(
                            |source: PtrConst, source_shape: &Shape, target: PtrUninit| {
                                if source_shape.is_type::<String>() {
                                    let source = unsafe { source.read::<String>() };
                                    let parsed = DateTime::parse_from_rfc3339(&source)
                                        .map(|dt| dt.with_timezone(&Local))
                                        .map_err(|_| ParseError::Generic("could not parse date"));
                                    match parsed {
                                        Ok(val) => Ok(unsafe { target.put(val) }),
                                        Err(_e) => Err(crate::TryFromError::Generic(
                                            "could not parse date",
                                        )),
                                    }
                                } else {
                                    Err(crate::TryFromError::UnsupportedSourceShape {
                                        src_shape: source_shape,
                                        expected: &[String::SHAPE],
                                    })
                                }
                            },
                        )
                    };
                    vtable.parse = {
                        Some(|s: &str, target: PtrUninit| {
                            let parsed = DateTime::parse_from_rfc3339(s)
                                .map(|dt| dt.with_timezone(&Local))
                                .map_err(|_| ParseError::Generic("could not parse date"))?;
                            Ok(unsafe { target.put(parsed) })
                        })
                    };
                    vtable.format.display = Some(|value, f| unsafe {
                        let dt = value.get::<DateTime<Local>>();
                        use chrono::SecondsFormat;
                        write!(f, "{}", dt.to_rfc3339_opts(SecondsFormat::Secs, true))
                    });
                }
                vtable
            },
            ty: Type::User(UserType::Opaque),
            def: Def::Scalar,
            type_identifier: "DateTime<Local>",
            type_params: &[],
            doc: &[],
            attributes: &[],
            type_tag: None,
            inner: None,
            proxy: None,
            variance: Variance::Invariant,
        }
    };
}

unsafe impl Facet<'_> for NaiveDateTime {
    const SHAPE: &'static Shape = &const {
        Shape {
            id: Shape::id_of::<Self>(),
            layout: Shape::layout_of::<Self>(),
            vtable: {
                let mut vtable = value_vtable!(NaiveDateTime, |f, _opts| write!(
                    f,
                    "{}",
                    Self::SHAPE.type_identifier
                ));
                {
                    vtable.try_from = {
                        Some(
                            |source: PtrConst, source_shape: &Shape, target: PtrUninit| {
                                if source_shape.is_type::<String>() {
                                    let source = unsafe { source.read::<String>() };
                                    let parsed =
                                        NaiveDateTime::parse_from_str(&source, "%Y-%m-%dT%H:%M:%S")
                                            .or_else(|_| {
                                                NaiveDateTime::parse_from_str(
                                                    &source,
                                                    "%Y-%m-%d %H:%M:%S",
                                                )
                                            })
                                            .map_err(|_| {
                                                ParseError::Generic("could not parse date")
                                            });
                                    match parsed {
                                        Ok(val) => Ok(unsafe { target.put(val) }),
                                        Err(_e) => Err(crate::TryFromError::Generic(
                                            "could not parse date",
                                        )),
                                    }
                                } else {
                                    Err(crate::TryFromError::UnsupportedSourceShape {
                                        src_shape: source_shape,
                                        expected: &[String::SHAPE],
                                    })
                                }
                            },
                        )
                    };
                    vtable.parse = {
                        Some(|s: &str, target: PtrUninit| {
                            let parsed = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S")
                                .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S"))
                                .map_err(|_| ParseError::Generic("could not parse date"))?;
                            Ok(unsafe { target.put(parsed) })
                        })
                    };
                    vtable.format.display = Some(|value, f| unsafe {
                        let dt = value.get::<NaiveDateTime>();
                        let formatted = dt.format("%Y-%m-%dT%H:%M:%S").to_string();
                        write!(f, "{formatted}")
                    });
                }
                vtable
            },
            ty: Type::User(UserType::Opaque),
            def: Def::Scalar,
            type_identifier: "NaiveDateTime",
            type_params: &[],
            doc: &[],
            attributes: &[],
            type_tag: None,
            inner: None,
            proxy: None,
            variance: Variance::Invariant,
        }
    };
}

unsafe impl Facet<'_> for NaiveDate {
    const SHAPE: &'static Shape = &const {
        Shape {
            id: Shape::id_of::<Self>(),
            layout: Shape::layout_of::<Self>(),
            vtable: {
                let mut vtable = value_vtable!(NaiveDate, |f, _opts| write!(
                    f,
                    "{}",
                    Self::SHAPE.type_identifier
                ));
                {
                    vtable.try_from = {
                        Some(
                            |source: PtrConst, source_shape: &Shape, target: PtrUninit| {
                                if source_shape.is_type::<String>() {
                                    let source = unsafe { source.read::<String>() };
                                    let parsed = NaiveDate::parse_from_str(&source, "%Y-%m-%d")
                                        .map_err(|_| ParseError::Generic("could not parse date"));
                                    match parsed {
                                        Ok(val) => Ok(unsafe { target.put(val) }),
                                        Err(_e) => Err(crate::TryFromError::Generic(
                                            "could not parse date",
                                        )),
                                    }
                                } else {
                                    Err(crate::TryFromError::UnsupportedSourceShape {
                                        src_shape: source_shape,
                                        expected: &[String::SHAPE],
                                    })
                                }
                            },
                        )
                    };
                    vtable.parse = {
                        Some(|s: &str, target: PtrUninit| {
                            let parsed = NaiveDate::parse_from_str(s, "%Y-%m-%d")
                                .map_err(|_| ParseError::Generic("could not parse date"))?;
                            Ok(unsafe { target.put(parsed) })
                        })
                    };
                    vtable.format.display = Some(|value, f| unsafe {
                        let dt = value.get::<NaiveDate>();
                        let formatted = dt.format("%Y-%m-%d").to_string();
                        write!(f, "{formatted}")
                    });
                }
                vtable
            },
            ty: Type::User(UserType::Opaque),
            def: Def::Scalar,
            type_identifier: "NaiveDate",
            type_params: &[],
            doc: &[],
            attributes: &[],
            type_tag: None,
            inner: None,
            proxy: None,
            variance: Variance::Invariant,
        }
    };
}

unsafe impl Facet<'_> for NaiveTime {
    const SHAPE: &'static Shape = &const {
        Shape {
            id: Shape::id_of::<Self>(),
            layout: Shape::layout_of::<Self>(),
            vtable: {
                let mut vtable = value_vtable!(NaiveTime, |f, _opts| write!(
                    f,
                    "{}",
                    Self::SHAPE.type_identifier
                ));
                {
                    vtable.try_from = {
                        Some(
                            |source: PtrConst, source_shape: &Shape, target: PtrUninit| {
                                if source_shape.is_type::<String>() {
                                    let source = unsafe { source.read::<String>() };
                                    let parsed = NaiveTime::parse_from_str(&source, "%H:%M:%S")
                                        .or_else(|_| {
                                            NaiveTime::parse_from_str(&source, "%H:%M:%S%.f")
                                        })
                                        .map_err(|_| ParseError::Generic("could not parse time"));
                                    match parsed {
                                        Ok(val) => Ok(unsafe { target.put(val) }),
                                        Err(_e) => Err(crate::TryFromError::Generic(
                                            "could not parse time",
                                        )),
                                    }
                                } else {
                                    Err(crate::TryFromError::UnsupportedSourceShape {
                                        src_shape: source_shape,
                                        expected: &[String::SHAPE],
                                    })
                                }
                            },
                        )
                    };
                    vtable.parse = {
                        Some(|s: &str, target: PtrUninit| {
                            let parsed = NaiveTime::parse_from_str(s, "%H:%M:%S")
                                .or_else(|_| NaiveTime::parse_from_str(s, "%H:%M:%S%.f"))
                                .map_err(|_| ParseError::Generic("could not parse time"))?;
                            Ok(unsafe { target.put(parsed) })
                        })
                    };
                    vtable.format.display = Some(|value, f| unsafe {
                        let dt = value.get::<NaiveTime>();
                        let formatted = dt.format("%H:%M:%S").to_string();
                        write!(f, "{formatted}")
                    });
                }
                vtable
            },
            ty: Type::User(UserType::Opaque),
            def: Def::Scalar,
            type_identifier: "NaiveTime",
            type_params: &[],
            doc: &[],
            attributes: &[],
            type_tag: None,
            inner: None,
            proxy: None,
            variance: Variance::Invariant,
        }
    };
}
