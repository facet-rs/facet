use facet::Facet;
use facet_json::from_str;

#[test]
fn test_struct_with_missing_field() -> eyre::Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Debug)]
    struct ThreeField {
        foo: String,
        bar: i32,
        baz: bool,
    }

    let json_data = r#"{"foo": "example", "bar": 100}"#;
    let result: Result<ThreeField, _> = from_str(json_data);
    let err = result.expect_err("Expected an error, but deserialization succeeded");
    insta::assert_debug_snapshot!(err);
    Ok(())
}

#[test]
fn test_deny_unknown_fields() -> eyre::Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Debug)]
    #[facet(deny_unknown_fields)]
    struct StrictStruct {
        foo: String,
        bar: i32,
    }

    // JSON with only expected fields
    let json_ok = r#"{"foo":"abc","bar":42}"#;
    let _strict: StrictStruct = from_str(json_ok)?;

    // JSON with an unexpected extra field should generate an error
    let json_extra = r#"{"foo":"abc","bar":42,"baz":true}"#;
    let result_extra: Result<StrictStruct, _> = from_str(json_extra);
    let err =
        result_extra.expect_err("Expected error for json_extra, but deserialization succeeded");
    insta::assert_debug_snapshot!(err);
    Ok(())
}

#[test]
#[ignore]
fn json_read_struct_level_default_unset_field() -> eyre::Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Default, Debug)]
    #[facet(default)]
    struct DefaultStruct {
        foo: i32,
        bar: String,
    }

    // Only set foo, leave bar missing - should use Default for String
    let json = r#"{"foo": 123}"#;

    let s: DefaultStruct = from_str(json)?;
    assert_eq!(s.foo, 123, "Expected foo to be 123, got {}", s.foo);
    assert!(
        s.bar.is_empty(),
        "Expected bar to be empty string, got {:?}",
        s.bar
    );
    Ok(())
}

#[test]
#[ignore]
fn json_read_field_level_default_no_function() -> eyre::Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Debug, PartialEq)]
    struct FieldDefault {
        foo: i32,
        #[facet(default)]
        bar: String,
    }

    // Only set foo, leave bar missing - should use Default for String
    let json = r#"{"foo": 789}"#;

    let s: FieldDefault = from_str(json)?;
    assert_eq!(s.foo, 789, "Expected foo to be 789, got {}", s.foo);
    assert_eq!(
        s.bar, "",
        "Expected bar to be empty string, got {:?}",
        s.bar
    );
    Ok(())
}

#[test]
#[ignore]
fn json_read_field_level_default_function() -> eyre::Result<()> {
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

    let s: FieldDefaultFn = from_str(json)?;
    assert_eq!(s.foo, 12345, "Expected foo to be 12345, got {}", s.foo);
    assert_eq!(s.bar, "hello", "Expected bar to be 'hello', got {}", s.bar);
    Ok(())
}

#[test]
fn test_allow_unknown_fields_1() -> eyre::Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Debug)]
    struct PermissiveStruct {
        foo: String,
        bar: i32,
    }

    // JSON with only expected fields
    let json_ok = r#"{"foo":"abc","bar":42}"#;
    let _ = from_str::<PermissiveStruct>(json_ok)?;

    // JSON with an unexpected extra field should NOT generate an error
    let json_extra = r#"{"foo":"abc","bar":42,"baz":[]}"#;
    let _ = from_str::<PermissiveStruct>(json_extra)?;
    Ok(())
}

#[test]
fn test_allow_unknown_fields_complex() -> eyre::Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Debug)]
    struct PermissiveStruct {
        foo: String,
        bar: i32,
    }

    // JSON with nested unknown objects and arrays
    let json_complex = r#"
    {
        "foo": "xyz",
        "bar": 99,
        "nested": {
            "a": 1,
            "b": [2, {"c":3}],
            "deep": {
                "x": {
                    "y": [true, false, {"z": null}]
                }
            }
        },
        "list": [
            {"inner": [1,2,3]},
            4,
            [{"more": "data"}]
        ]
    }
    "#;
    let result: PermissiveStruct = from_str(json_complex)?;
    assert_eq!(
        result.foo, "xyz",
        "Expected foo to be 'xyz', got {}",
        result.foo
    );
    assert_eq!(result.bar, 99, "Expected bar to be 99, got {}", result.bar);
    Ok(())
}
