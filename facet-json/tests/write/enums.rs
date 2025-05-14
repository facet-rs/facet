#[test]
fn enum_() {
    facet_testhelpers::setup();

    #[allow(dead_code)]
    #[derive(facet::Facet)]
    #[repr(C)]
    enum Point {
        Variant1 { field1: String, field2: String },
        Variant2(String),
        Variant3(String, String),
    }

    let good_point = Point::Variant1 {
        field1: "aaa".to_string(),
        field2: "bbb".to_string(),
    };
    assert_eq!(
        facet_json::to_string(&good_point),
        r#"{"Variant1":{"field1":"aaa","field2":"bbb"}}"#
    );

    let bad_point = Point::Variant2("aaa".to_string());
    assert_eq!(facet_json::to_string(&bad_point), r#"{"Variant2":"aaa"}"#);

    let medium_point = Point::Variant3("aaa".to_string(), "bbb".to_string());
    assert_eq!(
        facet_json::to_string(&medium_point),
        r#"{"Variant3":["aaa","bbb"]}"#
    );
}
