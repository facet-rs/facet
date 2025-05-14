use eyre::Result;
use facet_json::{from_str, to_string};

#[test]
fn test_deserialize_tuple_string() -> Result<()> {
    let ok: (String,) = from_str(r#"[""]"#)?;
    assert_eq!(ok.0, "");

    let ok: (String, String, String) = from_str(r#"["un","deux","trois"]"#)?;
    assert_eq!(ok.0, "un");
    assert_eq!(ok.1, "deux");
    assert_eq!(ok.2, "trois");

    let ok: (String, String, String) = from_str(r#"["🐑","🐑🐑","🐑🐑🐑"]"#)?;
    assert_eq!(ok.0, "🐑");
    assert_eq!(ok.1, "🐑🐑");
    assert_eq!(ok.2, "🐑🐑🐑");

    Ok(())
}

#[test]
fn test_deserialize_tuple_i32() -> Result<()> {
    let ok: (i32,) = from_str(r#"[10]"#)?;
    assert_eq!(ok.0, 10);

    let ok: (i32, i32) = from_str(r#"[10,20]"#)?;
    assert_eq!(ok.0, 10);
    assert_eq!(ok.1, 20);

    let ok: (i32, i32, i32) = from_str(r#"[10,20,30]"#)?;
    assert_eq!(ok.0, 10);
    assert_eq!(ok.1, 20);
    assert_eq!(ok.2, 30);

    let ok: (i32, i32, i32, i32) = from_str(r#"[10,20,30,40]"#)?;
    assert_eq!(ok.0, 10);
    assert_eq!(ok.1, 20);
    assert_eq!(ok.2, 30);
    assert_eq!(ok.3, 40);

    let ok: (i32, i32, i32, i32, i32) = from_str(r#"[10,20,30,40,50]"#)?;
    assert_eq!(ok.0, 10);
    assert_eq!(ok.1, 20);
    assert_eq!(ok.2, 30);
    assert_eq!(ok.3, 40);
    assert_eq!(ok.4, 50);

    let ok: (i32, i32) = from_str(r#"[-1,-0]"#)?;
    assert_eq!(ok.0, -1);
    assert_eq!(ok.1, 0);

    Ok(())
}

#[test]
fn test_deserialize_tuple_f32() -> Result<()> {
    let ok: (f32,) = from_str(r#"[10]"#)?;
    assert_eq!(ok.0, 10.0);

    let ok: (f32, f32) = from_str(r#"[10,20]"#)?;
    assert_eq!(ok.0, 10.0);
    assert_eq!(ok.1, 20.0);

    let ok: (f32, f32, f32) = from_str(r#"[10,20,30]"#)?;
    assert_eq!(ok.0, 10.0);
    assert_eq!(ok.1, 20.0);
    assert_eq!(ok.2, 30.0);

    let ok: (f32, f32, f32, f32) = from_str(r#"[10,20,30,40]"#)?;
    assert_eq!(ok.0, 10.0);
    assert_eq!(ok.1, 20.0);
    assert_eq!(ok.2, 30.0);
    assert_eq!(ok.3, 40.0);

    let ok: (f32, f32, f32, f32, f32) = from_str(r#"[10,20,30,40,50]"#)?;
    assert_eq!(ok.0, 10.0);
    assert_eq!(ok.1, 20.0);
    assert_eq!(ok.2, 30.0);
    assert_eq!(ok.3, 40.0);
    assert_eq!(ok.4, 50.0);

    let ok: (f32, f32) = from_str(r#"[-1,-0]"#)?;
    assert_eq!(ok.0, -1.0);
    assert_eq!(ok.1, 0.0);

    Ok(())
}

#[test]
fn test_deserialize_tuple_mixed_string_i32() -> Result<()> {
    let ok: (String, i32) = from_str(r#"["aaa",100]"#)?;
    assert_eq!(ok.0, "aaa");
    assert_eq!(ok.1, 100);

    Ok(())
}

#[test]
fn test_deserialize_tuple_mixed_i32_f32() -> Result<()> {
    let ok: (i32, f32) = from_str(r#"[10,20]"#)?;
    assert_eq!(ok.0, 10);
    assert_eq!(ok.1, 20.0);

    let ok: (f32, i32) = from_str(r#"[10,20]"#)?;
    assert_eq!(ok.0, 10.0);
    assert_eq!(ok.1, 20);

    let ok: (i32, f32, i32) = from_str(r#"[10,20,30]"#)?;
    assert_eq!(ok.0, 10);
    assert_eq!(ok.1, 20.0);
    assert_eq!(ok.2, 30);

    let ok: (f32, i32, f32, i32) = from_str(r#"[10,20,30,40]"#)?;
    assert_eq!(ok.0, 10.0);
    assert_eq!(ok.1, 20);
    assert_eq!(ok.2, 30.0);
    assert_eq!(ok.3, 40);

    Ok(())
}

#[test]
fn test_deserialize_tuple_empty_simple() -> Result<()> {
    let _ok: () = from_str(r#"[]"#)?;

    Ok(())
}

#[test]
fn test_deserialize_tuple_empty_nest() -> Result<()> {
    let _ok: ((),) = from_str(r#"[[]]"#)?;

    Ok(())
}

#[test]
fn test_deserialize_tuple_empty_nests() -> Result<()> {
    let _ok: ((), ()) = from_str(r#"[[],[]]"#)?;

    Ok(())
}

#[test]
fn test_deserialize_tuple_nest() -> Result<()> {
    let ok: ((String,),) = from_str(r#"[["hello"]]"#)?;
    assert_eq!(ok.0.0, "hello");

    type String1Tuple = (String,);
    type IntFloatString3Tuple = (i32, f32, String);

    let ok: (String1Tuple, IntFloatString3Tuple) = from_str(r#"[["hello"],[1,2,"3"]]"#)?;
    assert_eq!(ok.0.0, "hello");
    assert_eq!(ok.1.0, 1);
    assert_eq!(ok.1.1, 2.0);
    assert_eq!(ok.1.2, "3");

    Ok(())
}

// (uGGP:uP,uGP:uP,uP:uP) Not unit (great grandparent, grandparent, parent) --> unit parent
// (i.e. there is an implicit default, the value is filled without being present)
// NOTE: Silently coerces to the type
/// Expect a 1-tuple[1-tuple[0-tuple]] but it's a 1-tuple[0-tuple]
#[test]
fn test_deserialize_tuple_empty_nested_flexible() -> Result<()> {
    // Expect a 1x-nested 0-tuple - yup it's a 1x-nested 0-tuple
    let _ok: ((),) = from_str("[[]]")?; // Correct
    // Expect a 2x-nested 0-tuple - coerced from a 1x-nested 0-tuple
    let _ok: (((),),) = from_str("[[]]")?; // Should be from "[[[]]]" but OK
    // Expect a 3x-nested 0-tuple - coerced from a 1x-nested 0-tuple
    let _ok: ((((),),),) = from_str("[[]]")?; // Should be from "[[[[]]]]" but OK

    Ok(())
}

// (uGGP:uP) Not unit great grandparent --> unit parent
// (as for uGP:uP case)
// NOTE: Silently coerces to the type
#[test]
fn test_deserialize_tuple_empty_nested_2x_flexible() -> Result<()> {
    // Expect a 2x-nested 0-tuple - yup it's a 2x-nested 0-tuple
    let _ok: (((),),) = from_str("[[[]]]")?; // Correct
    // Expect a 3x-nested 0-tuple - coerced from a 2x-nested 0-tuple
    let _ok: ((((),),),) = from_str("[[[]]]")?; // Should be from "[[[[]]]]" but OK
    // Expect a 4x-nested 0-tuple - coerced from a 2x-nested 0-tuple
    let _ok: (((((),),),),) = from_str("[[[]]]")?; // Should be from "[[[[[]]]]]" but OK

    Ok(())
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
