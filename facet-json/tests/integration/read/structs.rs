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

#[test]
#[ignore]
fn test_reading_flat_structs() {
    facet_testhelpers::setup();

    #[derive(Debug, PartialEq, Eq, facet::Facet)]
    struct Outer {
        name: String,
        #[facet(flatten)]
        struct_: InnerStruct,
        #[facet(flatten)]
        enum_: InnerEnum,
    }

    #[derive(Debug, PartialEq, Eq, facet::Facet)]
    struct InnerStruct {
        val: u64,
    }

    #[derive(Debug, PartialEq, Eq, facet::Facet)]
    #[allow(dead_code)]
    #[repr(C)]
    enum InnerEnum {
        Variant1 { field1: String, field2: String },
        Variant2(String),
        Variant3(String, String),
    }

    let actual1: Outer = facet_json::from_str(
        r#"{"name":"test1","val":1,"Variant1":{"field1":"aaa","field2":"bbb"}}"#,
    )
    .unwrap();
    let expected1 = Outer {
        name: "test1".to_string(),
        struct_: InnerStruct { val: 1 },
        enum_: InnerEnum::Variant1 {
            field1: "aaa".to_string(),
            field2: "bbb".to_string(),
        },
    };
    assert_eq!(expected1, actual1);

    let actual2: Outer =
        facet_json::from_str(r#"{"name":"test1","val":2,"Variant2":"ccc"}"#).unwrap();
    let expected2 = Outer {
        name: "test2".to_string(),
        struct_: InnerStruct { val: 2 },
        enum_: InnerEnum::Variant2("ccc".to_string()),
    };
    assert_eq!(expected2, actual2);

    let actual3: Outer =
        facet_json::from_str(r#"{"name":"test1","val":3,"Variant3":["ddd","eee"]}"#).unwrap();
    let expected3 = Outer {
        name: "test3".to_string(),
        struct_: InnerStruct { val: 3 },
        enum_: InnerEnum::Variant3("ddd".to_string(), "eee".to_string()),
    };
    assert_eq!(expected3, actual3);
}
