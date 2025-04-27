use eyre::Result;
use facet::Facet;
use facet_json::from_str;

#[test]
fn json_read_struct_twofields() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet)]
    struct TestStruct {
        name: String,
        age: u64,
    }
    let json = r#"{"name": "Alice", "age": 30}"#;

    let s: TestStruct = from_str(json)?;
    assert_eq!(s.name, "Alice");
    assert_eq!(s.age, 30);

    Ok(())
}

#[test]
fn json_read_struct_threefields() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet)]
    struct TestStruct {
        name: String,
        age: u64,
        hobbies: Vec<String>,
    }
    let json = r#"{"name": "Alice", "age": 30, "hobbies": ["reading", "coding"]}"#;

    let s: TestStruct = from_str(json)?;
    assert_eq!(s.name, "Alice");
    assert_eq!(s.age, 30);
    assert_eq!(s.hobbies.len(), 2);
    assert_eq!(s.hobbies[0], "reading");
    assert_eq!(s.hobbies[1], "coding");

    Ok(())
}

#[test]
fn test_from_json_with_nested_structs() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet)]
    struct InnerStruct {
        value: u64,
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

    let test_struct: OuterStruct = from_str(json)?;

    assert_eq!(test_struct.name, "Outer");
    assert_eq!(test_struct.inner.value, 42);

    Ok(())
}
