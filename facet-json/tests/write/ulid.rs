use eyre::Result;
use facet::Facet;
use facet_json::to_string;
use ulid::Ulid;

#[test]
fn json_write_ulid() -> Result<()> {
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
