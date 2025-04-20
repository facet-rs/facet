#[test]
fn json_read_struct_twofields() {
    facet_testhelpers::setup();

    #[derive(Facet)]
    struct TestStruct {
        name: String,
        age: u64,
    }
    let json = r#"{"name": "Alice", "age": 30}"#;

    let s: TestStruct = match from_str(json) {
        Ok(s) => s,
        Err(e) => panic!("Error deserializing JSON: {}", e),
    };
    assert_eq!(s.name, "Alice");
    assert_eq!(s.age, 30);
}

#[test]
fn json_read_struct_threefields() {
    facet_testhelpers::setup();

    #[derive(Facet)]
    struct TestStruct {
        name: String,
        age: u64,
        hobbies: Vec<String>,
    }
    let json = r#"{"name": "Alice", "age": 30, "hobbies": ["reading", "coding"]}"#;

    let s: TestStruct = match from_str(json) {
        Ok(s) => s,
        Err(e) => panic!("Error deserializing JSON: {}", e),
    };
    assert_eq!(s.name, "Alice");
    assert_eq!(s.age, 30);
    assert_eq!(s.hobbies.len(), 2);
    assert_eq!(s.hobbies[0], "reading");
    assert_eq!(s.hobbies[1], "coding");
}

#[test]
fn json_read_nonzero() {
    facet_testhelpers::setup();

    #[derive(Facet)]
    struct Foo {
        foo: NonZero<u8>,
    }
    let json = r#"{"foo": 1}"#;
    let s: Foo = match from_str(json) {
        Ok(s) => s,
        Err(e) => panic!("Error deserializing JSON: {}", e),
    };
    assert_eq!(s.foo, { const { NonZero::new(1).unwrap() } });
}

#[test]
fn test_from_json_with_nested_structs() {
    facet_testhelpers::setup();

    #[derive(Facet)]
    struct InnerStruct {
        value: i32,
    }

    #[derive(Facet)]
    struct OuterStruct {
        name: String,
        inner: InnerStruct,
    }

    let json = r#"{
        "name": "Outer",
        "inner": {
            "value": 42
        }
    }"#;

    let test_struct: OuterStruct = match from_str(json) {
        Ok(s) => s,
        Err(e) => panic!("Error deserializing JSON: {}", e),
    };

    assert_eq!(test_struct.name, "Outer");
    assert_eq!(test_struct.inner.value, 42);
}
