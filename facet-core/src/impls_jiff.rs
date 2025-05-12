use alloc::string::String;
use jiff::{Timestamp, Zoned, civil::DateTime};

use crate::{
    Def, Facet, ParseError, PtrConst, PtrUninit, ScalarAffinity, ScalarDef, Shape, Type, UserType,
    ValueVTable, value_vtable,
};

const ZONED_ERROR: &str = "could not parse time-zone aware instant of time";

unsafe impl Facet<'_> for Zoned {
    const VTABLE: &'static ValueVTable = &const {
        let mut vtable = value_vtable!(Zoned, |f, _opts| write!(f, "Zoned"));
        vtable.try_from = Some(
            |source: PtrConst, source_shape: &Shape, target: PtrUninit| {
                if source_shape.is_type::<String>() {
                    let source = unsafe { source.read::<String>() };
                    let parsed = source
                        .parse::<Zoned>()
                        .map_err(|_| ParseError::Generic(ZONED_ERROR));
                    match parsed {
                        Ok(val) => Ok(unsafe { target.put(val) }),
                        Err(_e) => Err(crate::TryFromError::Generic(ZONED_ERROR)),
                    }
                } else {
                    Err(crate::TryFromError::UnsupportedSourceShape {
                        src_shape: source_shape,
                        expected: &[String::SHAPE],
                    })
                }
            },
        );
        vtable.parse = Some(|s: &str, target: PtrUninit| {
            let parsed: Zoned = s.parse().map_err(|_| ParseError::Generic(ZONED_ERROR))?;
            Ok(unsafe { target.put(parsed) })
        });
        vtable.display = Some(|value, f| unsafe { write!(f, "{}", value.get::<Zoned>()) });
        vtable
    };

    const SHAPE: &'static Shape = &const {
        Shape::builder_for_sized::<Self>()
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(ScalarAffinity::time().build())
                    .build(),
            ))
            .build()
    };
}

const TIMESTAMP_ERROR: &str = "could not parse timestamp";

unsafe impl Facet<'_> for Timestamp {
    const VTABLE: &'static ValueVTable = &const {
        let mut vtable = value_vtable!(Timestamp, |f, _opts| write!(f, "Timestamp"));
        vtable.try_from = Some(
            |source: PtrConst, source_shape: &Shape, target: PtrUninit| {
                if source_shape.is_type::<String>() {
                    let source = unsafe { source.read::<String>() };
                    let parsed = source
                        .parse::<Timestamp>()
                        .map_err(|_| ParseError::Generic(TIMESTAMP_ERROR));
                    match parsed {
                        Ok(val) => Ok(unsafe { target.put(val) }),
                        Err(_e) => Err(crate::TryFromError::Generic(TIMESTAMP_ERROR)),
                    }
                } else {
                    Err(crate::TryFromError::UnsupportedSourceShape {
                        src_shape: source_shape,
                        expected: &[String::SHAPE],
                    })
                }
            },
        );
        vtable.parse = Some(|s: &str, target: PtrUninit| {
            let parsed: Timestamp = s
                .parse()
                .map_err(|_| ParseError::Generic(TIMESTAMP_ERROR))?;
            Ok(unsafe { target.put(parsed) })
        });
        vtable.display = Some(|value, f| unsafe { write!(f, "{}", value.get::<Timestamp>()) });
        vtable
    };

    const SHAPE: &'static Shape = &const {
        Shape::builder_for_sized::<Self>()
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(ScalarAffinity::time().build())
                    .build(),
            ))
            .build()
    };
}

const DATETIME_ERROR: &str = "could not parse civil datetime";

unsafe impl Facet<'_> for DateTime {
    const VTABLE: &'static ValueVTable = &const {
        let mut vtable = value_vtable!(DateTime, |f, _opts| write!(f, "DateTime"));
        vtable.try_from = Some(
            |source: PtrConst, source_shape: &Shape, target: PtrUninit| {
                if source_shape.is_type::<String>() {
                    let source = unsafe { source.read::<String>() };
                    let parsed = source
                        .parse::<DateTime>()
                        .map_err(|_| ParseError::Generic(DATETIME_ERROR));
                    match parsed {
                        Ok(val) => Ok(unsafe { target.put(val) }),
                        Err(_e) => Err(crate::TryFromError::Generic(DATETIME_ERROR)),
                    }
                } else {
                    Err(crate::TryFromError::UnsupportedSourceShape {
                        src_shape: source_shape,
                        expected: &[String::SHAPE],
                    })
                }
            },
        );
        vtable.parse = Some(|s: &str, target: PtrUninit| {
            let parsed: DateTime = s.parse().map_err(|_| ParseError::Generic(DATETIME_ERROR))?;
            Ok(unsafe { target.put(parsed) })
        });
        vtable.display = Some(|value, f| unsafe { write!(f, "{}", value.get::<DateTime>()) });
        vtable
    };

    const SHAPE: &'static Shape = &const {
        Shape::builder_for_sized::<Self>()
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(ScalarAffinity::time().build())
                    .build(),
            ))
            .build()
    };
}

#[cfg(test)]
mod tests {
    use core::fmt;

    use jiff::{Timestamp, Zoned, civil::DateTime};

    use crate::{Facet, PtrConst};

    #[test]
    fn parse_zoned() -> eyre::Result<()> {
        facet_testhelpers::setup();

        let target = Zoned::SHAPE.allocate()?;
        unsafe {
            (Zoned::VTABLE.parse.unwrap())("2023-12-31T18:30:00+07:00[Asia/Ho_Chi_Minh]", target)?;
        }
        let odt: Zoned = unsafe { target.assume_init().read() };
        assert_eq!(odt, "2023-12-31T18:30:00+07:00[Asia/Ho_Chi_Minh]".parse()?);

        struct DisplayWrapper<'a>(PtrConst<'a>);

        impl fmt::Display for DisplayWrapper<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                unsafe { (Zoned::VTABLE.display.unwrap())(self.0, f) }
            }
        }

        let s = format!("{}", DisplayWrapper(PtrConst::new(&odt as *const _)));
        assert_eq!(s, "2023-12-31T18:30:00+07:00[Asia/Ho_Chi_Minh]");

        // Deallocate the heap allocation to avoid memory leaks under Miri
        unsafe {
            Zoned::SHAPE.deallocate_uninit(target)?;
        }

        Ok(())
    }

    #[test]
    fn parse_timestamp() -> eyre::Result<()> {
        facet_testhelpers::setup();

        let target = Timestamp::SHAPE.allocate()?;
        unsafe {
            (Timestamp::VTABLE.parse.unwrap())("2024-06-19T15:22:45Z", target)?;
        }
        let odt: Timestamp = unsafe { target.assume_init().read() };
        assert_eq!(odt, "2024-06-19T15:22:45Z".parse()?);

        struct DisplayWrapper<'a>(PtrConst<'a>);

        impl fmt::Display for DisplayWrapper<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                unsafe { (Timestamp::VTABLE.display.unwrap())(self.0, f) }
            }
        }

        let s = format!("{}", DisplayWrapper(PtrConst::new(&odt as *const _)));
        assert_eq!(s, "2024-06-19T15:22:45Z");

        // Deallocate the heap allocation to avoid memory leaks under Miri
        unsafe {
            Timestamp::SHAPE.deallocate_uninit(target)?;
        }

        Ok(())
    }

    #[test]
    fn parse_datetime() -> eyre::Result<()> {
        facet_testhelpers::setup();

        let target = DateTime::SHAPE.allocate()?;
        unsafe {
            (DateTime::VTABLE.parse.unwrap())("2024-06-19T15:22:45", target)?;
        }
        let odt: DateTime = unsafe { target.assume_init().read() };
        assert_eq!(odt, "2024-06-19T15:22:45".parse()?);

        struct DisplayWrapper<'a>(PtrConst<'a>);

        impl fmt::Display for DisplayWrapper<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                unsafe { (DateTime::VTABLE.display.unwrap())(self.0, f) }
            }
        }

        let s = format!("{}", DisplayWrapper(PtrConst::new(&odt as *const _)));
        assert_eq!(s, "2024-06-19T15:22:45");

        // Deallocate the heap allocation to avoid memory leaks under Miri
        unsafe {
            DateTime::SHAPE.deallocate_uninit(target)?;
        }

        Ok(())
    }
}
