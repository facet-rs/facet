use alloc::string::String;
use jiff::{Timestamp, Zoned, civil::DateTime};

use crate::{
    Def, Facet, ParseError, PtrConst, PtrUninit, Shape, Type, UserType, Variance, value_vtable,
};

const ZONED_ERROR: &str = "could not parse time-zone aware instant of time";

unsafe impl Facet<'_> for Zoned {
    const SHAPE: &'static Shape = &const {
        Shape {
            id: Shape::id_of::<Self>(),
            layout: Shape::layout_of::<Self>(),
            vtable: {
                let mut vtable = value_vtable!(Zoned, |f, _opts| write!(
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
                        )
                    };
                    vtable.parse = {
                        Some(|s: &str, target: PtrUninit| {
                            let parsed: Zoned =
                                s.parse().map_err(|_| ParseError::Generic(ZONED_ERROR))?;
                            Ok(unsafe { target.put(parsed) })
                        })
                    };
                    vtable.format.display =
                        Some(|value, f| unsafe { write!(f, "{}", value.get::<Zoned>()) });
                }
                vtable
            },
            ty: Type::User(UserType::Opaque),
            def: Def::Scalar,
            type_identifier: "Zoned",
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

const TIMESTAMP_ERROR: &str = "could not parse timestamp";

unsafe impl Facet<'_> for Timestamp {
    const SHAPE: &'static Shape = &const {
        Shape {
            id: Shape::id_of::<Self>(),
            layout: Shape::layout_of::<Self>(),
            vtable: {
                let mut vtable = value_vtable!(Timestamp, |f, _opts| write!(
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
                                    let parsed = source
                                        .parse::<Timestamp>()
                                        .map_err(|_| ParseError::Generic(TIMESTAMP_ERROR));
                                    match parsed {
                                        Ok(val) => Ok(unsafe { target.put(val) }),
                                        Err(_e) => {
                                            Err(crate::TryFromError::Generic(TIMESTAMP_ERROR))
                                        }
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
                            let parsed: Timestamp = s
                                .parse()
                                .map_err(|_| ParseError::Generic(TIMESTAMP_ERROR))?;
                            Ok(unsafe { target.put(parsed) })
                        })
                    };
                    vtable.format.display =
                        Some(|value, f| unsafe { write!(f, "{}", value.get::<Timestamp>()) });
                }
                vtable
            },
            ty: Type::User(UserType::Opaque),
            def: Def::Scalar,
            type_identifier: "Timestamp",
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

const DATETIME_ERROR: &str = "could not parse civil datetime";

unsafe impl Facet<'_> for DateTime {
    const SHAPE: &'static Shape = &const {
        Shape {
            id: Shape::id_of::<Self>(),
            layout: Shape::layout_of::<Self>(),
            vtable: {
                let mut vtable = value_vtable!(DateTime, |f, _opts| write!(
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
                                    let parsed = source
                                        .parse::<DateTime>()
                                        .map_err(|_| ParseError::Generic(DATETIME_ERROR));
                                    match parsed {
                                        Ok(val) => Ok(unsafe { target.put(val) }),
                                        Err(_e) => {
                                            Err(crate::TryFromError::Generic(DATETIME_ERROR))
                                        }
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
                            let parsed: DateTime =
                                s.parse().map_err(|_| ParseError::Generic(DATETIME_ERROR))?;
                            Ok(unsafe { target.put(parsed) })
                        })
                    };
                    vtable.format.display =
                        Some(|value, f| unsafe { write!(f, "{}", value.get::<DateTime>()) });
                }
                vtable
            },
            ty: Type::User(UserType::Opaque),
            def: Def::Scalar,
            type_identifier: "DateTime",
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

#[cfg(test)]
mod tests {
    use core::{fmt, ptr::NonNull};

    use jiff::{Timestamp, civil::DateTime};

    use crate::{Facet, PtrConst};

    #[test]
    #[cfg(not(miri))] // I don't think we can read time zones from miri, the test just fails
    fn parse_zoned() {
        use jiff::Zoned;

        facet_testhelpers::setup();

        let target = Zoned::SHAPE.allocate().unwrap();
        unsafe {
            Zoned::SHAPE.vtable.parse.unwrap()(
                "2023-12-31T18:30:00+07:00[Asia/Ho_Chi_Minh]",
                target,
            )
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
            struct DisplayWrapper<'a>(PtrConst<'a>);

            impl fmt::Display for DisplayWrapper<'_> {
                fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    unsafe { (Zoned::SHAPE.vtable.format.display.unwrap())(self.0, f) }
                }
            }

            let s = format!("{}", DisplayWrapper(PtrConst::new(NonNull::from(&odt))));
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
        unsafe {
            (Timestamp::SHAPE.vtable.parse.unwrap())("2024-06-19T15:22:45Z", target).unwrap();
        }
        let odt: Timestamp = unsafe { target.assume_init().read() };
        assert_eq!(odt, "2024-06-19T15:22:45Z".parse().unwrap());

        {
            struct DisplayWrapper<'a>(PtrConst<'a>);

            impl fmt::Display for DisplayWrapper<'_> {
                fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    unsafe { (Timestamp::SHAPE.vtable.format.display.unwrap())(self.0, f) }
                }
            }

            let s = format!("{}", DisplayWrapper(PtrConst::new(NonNull::from(&odt))));
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
        unsafe {
            (DateTime::SHAPE.vtable.parse.unwrap())("2024-06-19T15:22:45", target).unwrap();
        }
        let odt: DateTime = unsafe { target.assume_init().read() };
        assert_eq!(odt, "2024-06-19T15:22:45".parse().unwrap());

        {
            struct DisplayWrapper<'a>(PtrConst<'a>);

            impl fmt::Display for DisplayWrapper<'_> {
                fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    unsafe { (DateTime::SHAPE.vtable.format.display.unwrap())(self.0, f) }
                }
            }

            let s = format!("{}", DisplayWrapper(PtrConst::new(NonNull::from(&odt))));
            assert_eq!(s, "2024-06-19T15:22:45");
        }

        // Deallocate the heap allocation to avoid memory leaks under Miri
        unsafe {
            DateTime::SHAPE.deallocate_uninit(target).unwrap();
        }
    }
}
