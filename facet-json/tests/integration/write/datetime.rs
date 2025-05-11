use eyre::Result;
use facet::Facet;
use facet_json::to_string;
use time::OffsetDateTime;

#[test]
fn write_datetime() -> Result<()> {
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
