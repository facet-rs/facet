use facet_json::from_str;

#[test]
#[ignore]
fn test_deserialize_tuple() {
    let result: Result<(&str, i32), _> = from_str(r#"["aaa",3]"#);
    let ok = result.unwrap();
    assert_eq!(ok.0, "aaa");
    assert_eq!(ok.1, 3);

    #[derive(facet::Facet)]
    struct TestTuple(i32, String, bool);
    let result: Result<TestTuple, _> = from_str(r#"[3,"aaa",true]"#);
    let ok = result.unwrap();
    assert_eq!(ok.0, 3);
    assert_eq!(ok.1, "aaa");
    assert!(ok.2);
}

#[test]
fn test_deserialize_list() {
    let result: Result<Vec<i32>, _> = from_str(r#"[1,3]"#);
    let ok = result.unwrap();
    assert_eq!(ok[0], 1);
    assert_eq!(ok[1], 3);
}
