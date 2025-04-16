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

#[test]
fn bad_json_1() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let json = "}";
    let err = from_str::<Foo>(json).unwrap_err();
    insta::assert_snapshot!(err);
    Ok(())
}

#[test]
fn bad_json_2() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let json = " }";
    let err = from_str::<Foo>(json).unwrap_err();
    insta::assert_snapshot!(err);
    Ok(())
}

#[test]
fn bad_json_3() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let json = "\n}";
    let err = from_str::<Foo>(json).unwrap_err();
    insta::assert_snapshot!(err);
    Ok(())
}

#[test]
fn bad_json_4() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let json = "\n  }";
    let err = from_str::<Foo>(json).unwrap_err();
    insta::assert_snapshot!(err);
    Ok(())
}

#[test]
fn bad_json_5() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let json = "\n  }\n// and then some";
    let err = from_str::<Foo>(json).unwrap_err();
    insta::assert_snapshot!(err);
    Ok(())
}

#[test]
fn bad_json_6_string_as_number_subpath() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let json = r#"{"foo": 42, "bar": 42}"#;
    let err = from_str::<FooBar>(json).unwrap_err();
    insta::assert_snapshot!(err);
    Ok(())
}
