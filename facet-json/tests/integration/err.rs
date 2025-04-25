use facet::Facet;
use facet_json::from_str;

#[derive(Facet, Debug)]
struct Foo {
    foo: u32,
}

#[derive(Facet, Debug)]
struct FooBar {
    foo: u64,
    bar: String,
}

#[cfg(not(miri))]
#[test]
fn bad_json_1() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let json = "}";
    let err = from_str::<Foo>(json).unwrap_err();
    insta::assert_snapshot!(err);
    Ok(())
}

#[cfg(not(miri))]
#[test]
fn bad_json_2() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let json = " }";
    let err = from_str::<Foo>(json).unwrap_err();
    insta::assert_snapshot!(err);
    Ok(())
}

#[cfg(not(miri))]
#[test]
fn bad_json_3() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let json = "\n}";
    let err = from_str::<Foo>(json).unwrap_err();
    insta::assert_snapshot!(err);
    Ok(())
}

#[cfg(not(miri))]
#[test]
fn bad_json_4() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let json = "\n  }";
    let err = from_str::<Foo>(json).unwrap_err();
    insta::assert_snapshot!(err);
    Ok(())
}

#[cfg(not(miri))]
#[test]
fn bad_json_5() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let json = "\n  }\n// and then some";
    let err = from_str::<Foo>(json).unwrap_err();
    insta::assert_snapshot!(err);
    Ok(())
}

#[cfg(not(miri))]
#[test]
fn bad_json_6_string_as_number_subpath() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let json = r#"{"foo": 42, "bar": 42}"#;
    let err = from_str::<FooBar>(json).unwrap_err();
    insta::assert_snapshot!(err);
    Ok(())
}

#[cfg(not(miri))]
#[test]
fn unknown_field_with_rename() -> eyre::Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Debug)]
    #[facet(deny_unknown_fields)]
    struct RenamedFields {
        #[facet(rename = "new_name")]
        original_name: String,
    }

    // This should fail because "wrong_name" doesn't match either the original field name
    // or the renamed field name
    let json = r#"{"wrong_name": "value"}"#;
    let err = from_str::<RenamedFields>(json).unwrap_err();
    insta::assert_snapshot!(err);

    Ok(())
}
