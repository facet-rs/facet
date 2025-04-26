#[test]
fn test_writing_flat_structs() {
    facet_testhelpers::setup();

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
