use eyre::Result;
use facet::Facet;
use facet_json::{from_str, to_string};
use ulid::Ulid;

#[test]
fn ulid_read() -> Result<()> {
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
fn ulid_write() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        better_id: Ulid,
    }

    let value = FooBar {
        better_id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
    };

    let json = to_string(&value);
    assert_eq!(json, r#"{"better_id":"01ARZ3NDEKTSV4RRFFQ69G5FAV"}"#);

    Ok(())
}
