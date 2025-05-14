use facet_testhelpers::test;

#[test]
fn ulid_read() {
    #[derive(facet::Facet, Debug, PartialEq)]
    struct FooBar {
        id: ulid::Ulid,
    }

    let json = r#"{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAV"}"#;

    let s: FooBar = facet_json::from_str(json)?;
    assert_eq!(
        s,
        FooBar {
            id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
        }
    );
}

#[test]
fn ulid_write() {
    #[derive(facet::Facet, Debug, PartialEq)]
    struct FooBar {
        better_id: ulid::Ulid,
    }

    let value = FooBar {
        better_id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
    };

    let json = facet_json::to_string(&value);
    assert_eq!(json, r#"{"better_id":"01ARZ3NDEKTSV4RRFFQ69G5FAV"}"#);
}
