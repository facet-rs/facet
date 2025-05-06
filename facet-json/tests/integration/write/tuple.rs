#![cfg(feature = "std")]

use facet_json::to_string;

#[test]
fn test_serialize_tuple() {
    let test_tuple1 = ("groetjes", 3);
    let json = to_string(&test_tuple1);
    assert_eq!(json, r#"["groetjes",3]"#);

    #[derive(facet::Facet)]
    struct TestTuple(i32, String, bool);
    let test_tuple2 = TestTuple(3, "aaa".to_string(), true);
    let json = to_string(&test_tuple2);
    assert_eq!(json, r#"[3,"aaa",true]"#);
}
