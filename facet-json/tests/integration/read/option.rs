use facet::Facet;
use facet_json::from_str;

#[test]
fn test_from_json_with_option() {
    facet_testhelpers::setup();

    #[derive(Facet)]
    struct Options {
        name: Option<String>,
        age: Option<u32>,
        inner: Option<Inner>,
    }

    #[derive(Facet)]
    struct Inner {
        foo: i32,
    }

    let json = r#"{
    "name": "Alice",
    "age": null,
    "inner": {
        "foo": 42
    }
}"#;

    let test_struct: Options = match from_str(json) {
        Ok(s) => s,
        Err(e) => panic!("Error deserializing JSON: {}", e),
    };
    assert_eq!(test_struct.name.as_deref(), Some("Alice"));
    assert_eq!(test_struct.age, None);
    assert_eq!(test_struct.inner.as_ref().map(|i| i.foo), Some(42));
}
