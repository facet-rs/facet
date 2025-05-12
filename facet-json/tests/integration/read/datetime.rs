use eyre::Result;
use facet::Facet;
use facet_json::from_str;
use jiff::{Timestamp, Zoned, civil::DateTime};
use time::OffsetDateTime;

#[test]
fn read_time_datetime() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        created_at: OffsetDateTime,
    }

    use time::macros::datetime;

    let json = r#"{"created_at":"2023-01-15T12:34:56Z"}"#;

    let s: FooBar = from_str(json)?;
    assert_eq!(
        s,
        FooBar {
            created_at: datetime!(2023-01-15 12:34:56 UTC),
        }
    );

    Ok(())
}

#[test]
#[cfg(not(miri))] // I don't think we can read time zones from miri, the test just fails
fn read_jiff_zoned() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        created_at: Zoned,
    }

    let json = r#"{"created_at":"2023-12-31T18:30:00+07:00[Asia/Ho_Chi_Minh]"}"#;

    let s: FooBar = from_str(json)?;

    assert_eq!(
        s,
        FooBar {
            created_at: jiff::civil::date(2023, 12, 31)
                .at(18, 30, 0, 0)
                .in_tz("Asia/Ho_Chi_Minh")?
        }
    );

    Ok(())
}

#[test]
fn read_jiff_timestamp() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        created_at: Timestamp,
    }

    let json = r#"{"created_at":"2023-12-31T11:30:00Z"}"#;

    let s: FooBar = from_str(json)?;

    assert_eq!(
        s,
        FooBar {
            created_at: "2023-12-31T18:30:00+07:00[Asia/Ho_Chi_Minh]".parse()?,
        }
    );

    Ok(())
}

#[test]
fn read_jiff_datetime() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        created_at: DateTime,
    }

    let json = r#"{"created_at":"2024-06-19T15:22:45"}"#;

    let s: FooBar = from_str(json)?;

    assert_eq!(
        s,
        FooBar {
            created_at: "2024-06-19T15:22:45".parse()?,
        }
    );

    Ok(())
}
