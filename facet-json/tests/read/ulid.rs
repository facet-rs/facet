use eyre::Result;
use facet::Facet;
use facet_json::from_str;
use ulid::Ulid;

#[test]
fn json_read_ulid() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        id: Ulid,
    }

    let json = r#"{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAV"}"#;

    let s: FooBar = from_str(json)?;
    assert_eq!(
        s,
        FooBar {
            id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
        }
    );

    Ok(())
}

#[test]
fn json_write_ulid() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        id: Ulid,
    }

    let original = FooBar {
        id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
    };

    let _json = facet_json::to_string(&original);

    Ok(())
}
