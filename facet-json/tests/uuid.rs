use eyre::Result;
use facet::Facet;
use facet_json::from_str;
use uuid::Uuid;

#[test]
fn uuid_write() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        id: Uuid,
    }

    let original = FooBar {
        id: "f49e1d6c-7e95-4654-a861-8b66f94a623a".parse().unwrap(),
    };

    let _json = facet_json::to_string(&original);

    Ok(())
}

#[test]
fn uuid_read() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        id: Uuid,
    }

    let json = r#"{"id":"f49e1d6c-7e95-4654-a861-8b66f94a623a"}"#;

    let s: FooBar = from_str(json)?;
    assert_eq!(
        s,
        FooBar {
            id: "f49e1d6c-7e95-4654-a861-8b66f94a623a".parse().unwrap(),
        }
    );

    Ok(())
}
