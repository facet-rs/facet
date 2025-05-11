use time::{OffsetDateTime, UtcDateTime};

use crate::{
    Def, Facet, ParseError, PtrUninit, ScalarAffinity, ScalarDef, Shape, Type, UserType,
    ValueVTable, value_vtable,
};

unsafe impl Facet<'_> for UtcDateTime {
    const VTABLE: &'static ValueVTable =
        &const { value_vtable!(UtcDateTime, |f, _opts| write!(f, "UtcDateTime")) };

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
            // Use the correct pattern for the `time` crate.
            // RFC 3339 is the format of "2023-03-14T15:09:26Z"
            let parsed = OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339)
                .map_err(|_| ParseError::Generic("could not parse date"))?;
            Ok(unsafe { target.put(parsed) })
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
    use time::OffsetDateTime;

    use crate::Facet;

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

        Ok(())
    }
}
