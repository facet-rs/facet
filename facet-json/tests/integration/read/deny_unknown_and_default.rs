use facet::Facet;
use facet_json::from_str;

#[test]
#[ignore]
fn test_deny_unknown_fields() {
    facet_testhelpers::setup();

    #[derive(Facet, Debug)]
    #[facet(deny_unknown_fields)]
    struct StrictStruct {
        foo: String,
        bar: i32,
    }

    // JSON with only expected fields
    let json_ok = r#"{"foo":"abc","bar":42}"#;
    let result_ok: Result<StrictStruct, _> = from_str(json_ok);
    assert!(result_ok.is_ok());

    // JSON with an unexpected extra field should generate an error
    let json_extra = r#"{"foo":"abc","bar":42,"baz":true}"#;
    let result_extra: Result<StrictStruct, _> = from_str(json_extra);
    assert!(result_extra.is_err());
}

#[test]
#[ignore]
fn json_read_struct_level_default_unset_field() {
    facet_testhelpers::setup();

    #[derive(Facet, Default, Debug)]
    #[facet(default)]
    struct DefaultStruct {
        foo: i32,
        bar: String,
    }

    // Only set foo, leave bar missing - should use Default for String
    let json = r#"{"foo": 123}"#;

    let s: DefaultStruct = match from_str(json) {
        Ok(s) => s,
        Err(e) => panic!("Error deserializing JSON: {}", e),
    };

    // bar should be the default String ("")
    assert_eq!(s.foo, 123);
    assert_eq!(s.bar, "");
}

#[test]
#[ignore]
fn json_read_field_level_default_no_function() {
    facet_testhelpers::setup();

    #[derive(Facet, Debug, PartialEq)]
    struct FieldDefault {
        foo: i32,
        #[facet(default)]
        bar: String,
    }

    // Only set foo, leave bar missing - should use Default for String
    let json = r#"{"foo": 789}"#;

    let s: FieldDefault = match from_str(json) {
        Ok(s) => s,
        Err(e) => panic!("Error deserializing JSON: {}", e),
    };

    assert_eq!(s.foo, 789);
    assert_eq!(s.bar, "");
}

#[test]
#[ignore]
fn json_read_field_level_default_function() {
    facet_testhelpers::setup();

    fn default_number() -> i32 {
        12345
    }

    #[derive(Facet, Debug, PartialEq)]
    struct FieldDefaultFn {
        #[facet(default = "default_number")]
        foo: i32,
        bar: String,
    }

    // Only set bar, leave foo missing - should use default_number()
    let json = r#"{"bar": "hello"}"#;

    let s: FieldDefaultFn = match from_str(json) {
        Ok(s) => s,
        Err(e) => panic!("Error deserializing JSON: {}", e),
    };

    assert_eq!(s.foo, 12345);
    assert_eq!(s.bar, "hello");
}

#[test]
#[ignore]
fn test_allow_unknown_fields() {
    facet_testhelpers::setup();

    #[derive(Facet, Debug)]
    struct PermissiveStruct {
        foo: String,
        bar: i32,
    }

    // JSON with only expected fields
    let json_ok = r#"{"foo":"abc","bar":42}"#;
    let result_ok: Result<PermissiveStruct, _> = from_str(json_ok);
    result_ok.unwrap();

    // JSON with an unexpected extra field should NOT generate an error
    let json_extra = r#"{"foo":"abc","bar":42,"baz":[]}"#;
    let result_extra: Result<PermissiveStruct, _> = from_str(json_extra);
    result_extra.unwrap();
}
