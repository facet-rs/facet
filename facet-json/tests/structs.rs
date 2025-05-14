use facet_testhelpers::test;

#[test]
fn json_read_struct_twofields() {
    #[derive(facet::Facet)]
    struct TestStruct {
        name: String,
        age: u64,
    }
    let json = r#"{"name": "Alice", "age": 30}"#;

    let s: TestStruct = facet_json::from_str(json).expect("Failed to parse JSON"); // Changed to expect to avoid ? causing implicit Result<()>
    assert_eq!(s.name, "Alice");
    assert_eq!(s.age, 30);
}

#[test]
fn json_read_struct_threefields() {
    #[derive(facet::Facet)]
    struct TestStruct {
        name: String,
        age: u64,
        hobbies: Vec<String>,
    }
    let json = r#"{"name": "Alice", "age": 30, "hobbies": ["reading", "coding"]}"#;

    let s: TestStruct = facet_json::from_str(json).expect("Failed to parse JSON"); // Changed to expect to avoid ? causing implicit Result<()>
    assert_eq!(s.name, "Alice");
    assert_eq!(s.age, 30);
    assert_eq!(s.hobbies.len(), 2);
    assert_eq!(s.hobbies[0], "reading");
    assert_eq!(s.hobbies[1], "coding");
}

#[test]
fn test_from_json_with_nested_structs() {
    #[derive(facet::Facet)]
    struct InnerStruct {
        value: u64,
    }

    #[derive(facet::Facet)]
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

    let test_struct: OuterStruct = facet_json::from_str(json).expect("Failed to parse JSON"); // Changed to expect to avoid ? causing implicit Result<()>

    assert_eq!(test_struct.name, "Outer");
    assert_eq!(test_struct.inner.value, 42);
}

#[test]
fn test_reading_flat_structs() {
    #[derive(Debug, PartialEq, Eq, facet::Facet)]
    struct Outer {
        name: String,
        #[facet(flatten)]
        struct_: InnerStruct,
        // #[facet(flatten)]
        // enum_: InnerEnum,
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
    .expect("Failed to parse JSON 1"); // Changed to expect to avoid ? causing implicit Result<()>; Unwraps were on Result in original
    let expected1 = Outer {
        name: "test1".to_string(),
        struct_: InnerStruct { val: 1 },
        // enum_: InnerEnum::Variant1 {
        //     field1: "aaa".to_string(),
        //     field2: "bbb".to_string(),
        // },
    };
    assert_eq!(expected1, actual1);

    let actual2: Outer = facet_json::from_str(r#"{"name":"test2","val":2,"Variant2":"ccc"}"#)
        .expect("Failed to parse JSON 2"); // Changed to expect to avoid ? causing implicit Result<()>; Unwraps were on Result in original
    let expected2 = Outer {
        name: "test2".to_string(),
        struct_: InnerStruct { val: 2 },
        // enum_: InnerEnum::Variant2("ccc".to_string()),
    };
    assert_eq!(expected2, actual2);

    let actual3: Outer =
        facet_json::from_str(r#"{"name":"test3","val":3,"Variant3":["ddd","eee"]}"#)
            .expect("Failed to parse JSON 3"); // Changed to expect to avoid ? causing implicit Result<()>; Unwraps were on Result in original
    let expected3 = Outer {
        name: "test3".to_string(),
        struct_: InnerStruct { val: 3 },
        // enum_: InnerEnum::Variant3("ddd".to_string(), "eee".to_string()),
    };
    assert_eq!(expected3, actual3);
}

#[test]
fn test_writing_flat_structs() {
    #[derive(facet::Facet)]
    struct Outer {
        name: &'static str,
        #[facet(flatten)]
        struct_: InnerStruct,
        #[facet(flatten)]
        enum_: InnerEnum,
    }

    #[derive(facet::Facet)]
    struct InnerStruct {
        val: u64,
    }
    #[derive(facet::Facet)]
    #[allow(dead_code)]
    #[repr(C)]
    enum InnerEnum {
        Variant1 { field1: String, field2: String },
        Variant2(String),
        Variant3(String, String),
    }
    let expected1 = r#"{"name":"test1","val":1,"Variant1":{"field1":"aaa","field2":"bbb"}}"#;
    let actual1 = facet_json::to_string(&Outer {
        name: "test1",
        struct_: InnerStruct { val: 1 },
        enum_: InnerEnum::Variant1 {
            field1: "aaa".to_string(),
            field2: "bbb".to_string(),
        },
    });
    assert_eq!(expected1, actual1);

    let expected2 = r#"{"name":"test2","val":2,"Variant2":"ccc"}"#;
    let actual2 = facet_json::to_string(&Outer {
        name: "test2",
        struct_: InnerStruct { val: 2 },
        enum_: InnerEnum::Variant2("ccc".to_string()),
    });
    assert_eq!(expected2, actual2);

    let expected3 = r#"{"name":"test3","val":3,"Variant3":["ddd","eee"]}"#;
    let actual3 = facet_json::to_string(&Outer {
        name: "test3",
        struct_: InnerStruct { val: 3 },
        enum_: InnerEnum::Variant3("ddd".to_string(), "eee".to_string()),
    });
    assert_eq!(expected3, actual3);
}
