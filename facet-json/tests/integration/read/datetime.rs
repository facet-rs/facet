use eyre::Result;
use facet::Facet;
use facet_json::from_str;
use jiff::Zoned;
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
fn write_jiff_zoned() -> Result<()> {
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
