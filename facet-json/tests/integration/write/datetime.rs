use eyre::Result;
use facet::Facet;
use facet_json::to_string;
use jiff::{Timestamp, Zoned, civil::DateTime};
use time::OffsetDateTime;

#[test]
fn write_time_datetime() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        created_at: OffsetDateTime,
    }

    use time::macros::datetime;

    let value = FooBar {
        created_at: datetime!(2023-01-15 12:34:56 UTC),
    };

    let json = to_string(&value);
    assert_eq!(json, r#"{"created_at":"2023-01-15T12:34:56Z"}"#);

    Ok(())
}

#[test]
fn write_jiff_zoned() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        created_at: Zoned,
    }

    let value = FooBar {
        created_at: jiff::civil::date(2023, 12, 31)
            .at(18, 30, 0, 0)
            .in_tz("Asia/Ho_Chi_Minh")?,
    };

    let json = to_string(&value);
    assert_eq!(
        json,
        r#"{"created_at":"2023-12-31T18:30:00+07:00[Asia/Ho_Chi_Minh]"}"#
    );

    Ok(())
}

#[test]
fn write_jiff_timestamp() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        created_at: Timestamp,
    }

    let value = FooBar {
        created_at: "2023-12-31T18:30:00+07:00[Asia/Ho_Chi_Minh]".parse()?,
    };

    let json = to_string(&value);
    assert_eq!(json, r#"{"created_at":"2023-12-31T11:30:00Z"}"#);

    Ok(())
}

#[test]
fn write_jiff_datetime() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        created_at: DateTime,
    }

    let value = FooBar {
        created_at: "2024-06-19T15:22:45".parse()?,
    };

    let json = to_string(&value);
    assert_eq!(json, r#"{"created_at":"2024-06-19T15:22:45"}"#);

    Ok(())
}
