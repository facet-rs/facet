use eyre::Result;
use facet::Facet;
use facet_json::to_string;
use uuid::Uuid;

#[test]
fn json_write_uuid() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        id: Uuid,
    }

    let value = FooBar {
        id: "f49e1d6c-7e95-4654-a861-8b66f94a623a".parse().unwrap(),
    };

    let json = to_string(&value);
    assert_eq!(json, r#"{"id":"f49e1d6c-7e95-4654-a861-8b66f94a623a"}"#);

    Ok(())
}
