use time::{OffsetDateTime, UtcDateTime};

use crate::{
    Def, Facet, ParseError, PtrUninit, ScalarAffinity, ScalarDef, Shape, Type, UserType,
    ValueVTable, value_vtable,
};

unsafe impl Facet<'_> for UtcDateTime {
    const VTABLE: &'static ValueVTable = &const {
        let mut vtable = value_vtable!(UtcDateTime, |f, _opts| write!(f, "UtcDateTime"));
        vtable.parse = Some(|s: &str, target: PtrUninit| {
            let parsed = UtcDateTime::parse(s, &time::format_description::well_known::Rfc3339)
                .map_err(|_| ParseError::Generic("could not parse date"))?;
            Ok(unsafe { target.put(parsed) })
        });
        vtable.display = Some(|value, f| unsafe {
            let udt = value.get::<UtcDateTime>();
            match udt.format(&time::format_description::well_known::Rfc3339) {
                Ok(s) => write!(f, "{s}"),
                Err(_) => write!(f, "<invalid UtcDateTime>"),
            }
        });
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

unsafe impl Facet<'_> for OffsetDateTime {
    const VTABLE: &'static ValueVTable = &const {
        let mut vtable = value_vtable!(OffsetDateTime, |f, _opts| write!(f, "OffsetDateTime"));
        vtable.parse = Some(|s: &str, target: PtrUninit| {
            let parsed = OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339)
                .map_err(|_| ParseError::Generic("could not parse date"))?;
            Ok(unsafe { target.put(parsed) })
        });
        vtable.display = Some(|value, f| unsafe {
            let odt = value.get::<OffsetDateTime>();
            match odt.format(&time::format_description::well_known::Rfc3339) {
                Ok(s) => write!(f, "{s}"),
                Err(_) => write!(f, "<invalid OffsetDateTime>"),
            }
        });
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

    use time::OffsetDateTime;

    use crate::{Facet, PtrConst};

    #[test]
    fn parse_offset_date_time() -> eyre::Result<()> {
        facet_testhelpers::setup();

        let target = OffsetDateTime::SHAPE.allocate()?;
        unsafe {
            (OffsetDateTime::VTABLE.parse.unwrap())("2023-03-14T15:09:26Z", target)?;
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

        struct DisplayWrapper<'a>(PtrConst<'a>);

        impl fmt::Display for DisplayWrapper<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                unsafe { (OffsetDateTime::VTABLE.display.unwrap())(self.0, f) }
            }
        }

        let s = format!("{}", DisplayWrapper(PtrConst::new(&odt as *const _)));
        assert_eq!(s, "2023-03-14T15:09:26Z");

        Ok(())
    }
}
