use eyre::Result;
use facet::Facet;
use facet_json::from_str;
use time::OffsetDateTime;

#[test]
fn json_read_datetime() -> Result<()> {
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
