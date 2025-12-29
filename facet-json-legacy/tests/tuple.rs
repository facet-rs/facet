use facet_json_legacy::{from_str, to_string};
use facet_testhelpers::test;

#[test]
fn test_deserialize_tuple_string() {
    let ok: (String,) = from_str(r#"[""]"#).unwrap();
    assert_eq!(ok.0, "");

    let ok: (String, String, String) = from_str(r#"["un","deux","trois"]"#).unwrap();
    assert_eq!(ok.0, "un");
    assert_eq!(ok.1, "deux");
    assert_eq!(ok.2, "trois");

    let ok: (String, String, String) = from_str(r#"["🐑","🐑🐑","🐑🐑🐑"]"#).unwrap();
    assert_eq!(ok.0, "🐑");
    assert_eq!(ok.1, "🐑🐑");
    assert_eq!(ok.2, "🐑🐑🐑");
}

#[test]
fn test_deserialize_tuple_i32() {
    let ok: (i32,) = from_str(r#"[10]"#).unwrap();
    assert_eq!(ok.0, 10);

    let ok: (i32, i32) = from_str(r#"[10,20]"#).unwrap();
    assert_eq!(ok.0, 10);
    assert_eq!(ok.1, 20);

    let ok: (i32, i32, i32) = from_str(r#"[10,20,30]"#).unwrap();
    assert_eq!(ok.0, 10);
    assert_eq!(ok.1, 20);
    assert_eq!(ok.2, 30);

    let ok: (i32, i32, i32, i32) = from_str(r#"[10,20,30,40]"#).unwrap();
    assert_eq!(ok.0, 10);
    assert_eq!(ok.1, 20);
    assert_eq!(ok.2, 30);
    assert_eq!(ok.3, 40);

    let ok: (i32, i32) = from_str(r#"[-1,-0]"#).unwrap();
    assert_eq!(ok.0, -1);
    assert_eq!(ok.1, 0);
}

#[test]
fn test_deserialize_tuple_f32() {
    let ok: (f32,) = from_str(r#"[10]"#).unwrap();
    assert_eq!(ok.0, 10.0);

    let ok: (f32, f32) = from_str(r#"[10,20]"#).unwrap();
    assert_eq!(ok.0, 10.0);
    assert_eq!(ok.1, 20.0);

    let ok: (f32, f32, f32) = from_str(r#"[10,20,30]"#).unwrap();
    assert_eq!(ok.0, 10.0);
    assert_eq!(ok.1, 20.0);
    assert_eq!(ok.2, 30.0);

    let ok: (f32, f32, f32, f32) = from_str(r#"[10,20,30,40]"#).unwrap();
    assert_eq!(ok.0, 10.0);
    assert_eq!(ok.1, 20.0);
    assert_eq!(ok.2, 30.0);
    assert_eq!(ok.3, 40.0);

    let ok: (f32, f32) = from_str(r#"[-1,-0]"#).unwrap();
    assert_eq!(ok.0, -1.0);
    assert_eq!(ok.1, 0.0);
}

#[test]
fn test_deserialize_tuple_mixed_string_i32() {
    let ok: (String, i32) = from_str(r#"["aaa",100]"#).unwrap();
    assert_eq!(ok.0, "aaa");
    assert_eq!(ok.1, 100);
}

#[test]
fn test_deserialize_tuple_mixed_i32_f32() {
    let ok: (i32, f32) = from_str(r#"[10,20]"#).unwrap();
    assert_eq!(ok.0, 10);
    assert_eq!(ok.1, 20.0);

    let ok: (f32, i32) = from_str(r#"[10,20]"#).unwrap();
    assert_eq!(ok.0, 10.0);
    assert_eq!(ok.1, 20);

    let ok: (i32, f32, i32) = from_str(r#"[10,20,30]"#).unwrap();
    assert_eq!(ok.0, 10);
    assert_eq!(ok.1, 20.0);
    assert_eq!(ok.2, 30);

    let ok: (f32, i32, f32, i32) = from_str(r#"[10,20,30,40]"#).unwrap();
    assert_eq!(ok.0, 10.0);
    assert_eq!(ok.1, 20);
    assert_eq!(ok.2, 30.0);
    assert_eq!(ok.3, 40);
}

#[test]
fn test_deserialize_tuple_empty_simple() {
    let _ok: () = from_str(r#"[]"#).unwrap();
}

#[test]
fn test_deserialize_tuple_empty_nest() {
    let _ok: ((),) = from_str(r#"[[]]"#).unwrap();
}

#[test]
fn test_deserialize_tuple_empty_nests() {
    let _ok: ((), ()) = from_str(r#"[[],[]]"#).unwrap();
}

#[test]
fn test_deserialize_tuple_nest() {
    let ok: ((String,),) = from_str(r#"[["hello"]]"#).unwrap();
    assert_eq!(ok.0.0, "hello");

    type String1Tuple = (String,);
    type IntFloatString3Tuple = (i32, f32, String);

    let ok: (String1Tuple, IntFloatString3Tuple) = from_str(r#"[["hello"],[1,2,"3"]]"#).unwrap();
    assert_eq!(ok.0.0, "hello");
    assert_eq!(ok.1.0, 1);
    assert_eq!(ok.1.1, 2.0);
    assert_eq!(ok.1.2, "3");
}

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
