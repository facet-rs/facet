use facet_json::from_str;

#[test]
fn test_deserialize_tuple_string() {
    let result: Result<(String,), _> = from_str(r#"[""]"#);
    let ok = result.unwrap();
    assert_eq!(ok.0, "");

    let result: Result<(String, String, String), _> = from_str(r#"["un","deux","trois"]"#);
    let ok = result.unwrap();
    assert_eq!(ok.0, "un");
    assert_eq!(ok.1, "deux");
    assert_eq!(ok.2, "trois");

    let result: Result<(String, String, String), _> = from_str(r#"["🐑","🐑🐑","🐑🐑🐑"]"#);
    let ok = result.unwrap();
    assert_eq!(ok.0, "🐑");
    assert_eq!(ok.1, "🐑🐑");
    assert_eq!(ok.2, "🐑🐑🐑");
}

#[test]
fn test_deserialize_tuple_i32() {
    let result: Result<(i32,), _> = from_str(r#"[10]"#);
    let ok = result.unwrap();
    assert_eq!(ok.0, 10);

    let result: Result<(i32, i32), _> = from_str(r#"[10,20]"#);
    let ok = result.unwrap();
    assert_eq!(ok.0, 10);
    assert_eq!(ok.1, 20);

    let result: Result<(i32, i32, i32), _> = from_str(r#"[10,20,30]"#);
    let ok = result.unwrap();
    assert_eq!(ok.0, 10);
    assert_eq!(ok.1, 20);
    assert_eq!(ok.2, 30);

    let result: Result<(i32, i32, i32, i32), _> = from_str(r#"[10,20,30,40]"#);
    let ok = result.unwrap();
    assert_eq!(ok.0, 10);
    assert_eq!(ok.1, 20);
    assert_eq!(ok.2, 30);
    assert_eq!(ok.3, 40);

    let result: Result<(i32, i32, i32, i32, i32), _> = from_str(r#"[10,20,30,40,50]"#);
    let ok = result.unwrap();
    assert_eq!(ok.0, 10);
    assert_eq!(ok.1, 20);
    assert_eq!(ok.2, 30);
    assert_eq!(ok.3, 40);
    assert_eq!(ok.4, 50);

    let result: Result<(i32, i32), _> = from_str(r#"[-1,-0]"#);
    let ok = result.unwrap();
    assert_eq!(ok.0, -1);
    assert_eq!(ok.1, 0);
}

#[test]
fn test_deserialize_tuple_f32() {
    let result: Result<(f32,), _> = from_str(r#"[10]"#);
    let ok = result.unwrap();
    assert_eq!(ok.0, 10.0);

    let result: Result<(f32, f32), _> = from_str(r#"[10,20]"#);
    let ok = result.unwrap();
    assert_eq!(ok.0, 10.0);
    assert_eq!(ok.1, 20.0);

    let result: Result<(f32, f32, f32), _> = from_str(r#"[10,20,30]"#);
    let ok = result.unwrap();
    assert_eq!(ok.0, 10.0);
    assert_eq!(ok.1, 20.0);
    assert_eq!(ok.2, 30.0);

    let result: Result<(f32, f32, f32, f32), _> = from_str(r#"[10,20,30,40]"#);
    let ok = result.unwrap();
    assert_eq!(ok.0, 10.0);
    assert_eq!(ok.1, 20.0);
    assert_eq!(ok.2, 30.0);
    assert_eq!(ok.3, 40.0);

    let result: Result<(f32, f32, f32, f32, f32), _> = from_str(r#"[10,20,30,40,50]"#);
    let ok = result.unwrap();
    assert_eq!(ok.0, 10.0);
    assert_eq!(ok.1, 20.0);
    assert_eq!(ok.2, 30.0);
    assert_eq!(ok.3, 40.0);
    assert_eq!(ok.4, 50.0);

    let result: Result<(f32, f32), _> = from_str(r#"[-1,-0]"#);
    let ok = result.unwrap();
    assert_eq!(ok.0, -1.0);
    assert_eq!(ok.1, 0.0);
}

#[test]
fn test_deserialize_tuple_mixed_string_i32() {
    let result: Result<(String, i32), _> = from_str(r#"["aaa",100]"#);
    let ok = result.unwrap();
    assert_eq!(ok.0, "aaa");
    assert_eq!(ok.1, 100);
}

#[test]
fn test_deserialize_tuple_mixed_i32_f32() {
    let result: Result<(i32, f32), _> = from_str(r#"[10,20]"#);
    let ok = result.unwrap();
    assert_eq!(ok.0, 10);
    assert_eq!(ok.1, 20.0);

    let result: Result<(f32, i32), _> = from_str(r#"[10,20]"#);
    let ok = result.unwrap();
    assert_eq!(ok.0, 10.0);
    assert_eq!(ok.1, 20);

    let result: Result<(i32, f32, i32), _> = from_str(r#"[10,20,30]"#);
    let ok = result.unwrap();
    assert_eq!(ok.0, 10);
    assert_eq!(ok.1, 20.0);
    assert_eq!(ok.2, 30);

    let result: Result<(f32, i32, f32, i32), _> = from_str(r#"[10,20,30,40]"#);
    let ok = result.unwrap();
    assert_eq!(ok.0, 10.0);
    assert_eq!(ok.1, 20);
    assert_eq!(ok.2, 30.0);
    assert_eq!(ok.3, 40);
}

#[test]
#[ignore]
fn test_deserialize_tuple_empty() {
    let result: Result<((), ()), _> = from_str(r#"[[],[]]"#);
    result.unwrap();
}

#[test]
fn test_deserialize_tuple_nest() {
    let result: Result<((String,),), _> = from_str(r#"[["hello"]]"#);
    let ok = result.unwrap();
    assert_eq!(ok.0.0, "hello");

    type String1Tuple = (String,);
    type IntFloatString3Tuple = (i32, f32, String);

    let result: Result<(String1Tuple, IntFloatString3Tuple), _> =
        from_str(r#"[["hello"],[1,2,"3"]]"#);
    let ok = result.unwrap();
    assert_eq!(ok.0.0, "hello");
    assert_eq!(ok.1.0, 1);
    assert_eq!(ok.1.1, 2.0);
    assert_eq!(ok.1.2, "3");
}
