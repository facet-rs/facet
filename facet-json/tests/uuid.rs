use facet::Facet;
use facet_json::{from_str, to_string};
use facet_testhelpers::test;
use uuid::Uuid;

#[test]
fn uuid_write() {
    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        id: Uuid,
    }

    let original = FooBar {
        id: "f49e1d6c-7e95-4654-a861-8b66f94a623a".parse()?,
    };

    let _json = to_string(&original);
}

#[test]
fn uuid_read() {
    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        id: Uuid,
    }

    let json = r#"{"id":"f49e1d6c-7e95-4654-a861-8b66f94a623a"}"#;

    let s: FooBar = from_str(json)?;
    assert_eq!(
        s,
        FooBar {
            id: "f49e1d6c-7e95-4654-a861-8b66f94a623a".parse()?,
        }
    );
}
