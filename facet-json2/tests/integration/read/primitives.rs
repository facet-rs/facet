use std::collections::HashMap;

use eyre::Result;
use facet::Facet;
use facet_json2::from_str;

#[test]
fn json_read_empty_object_for_struct() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet)]
    struct TestStruct {}
    let json = r#"{}"#;

    let _: TestStruct = from_str(json)?;
    Ok(())
}

#[test]
fn json_read_empty_object_for_hashmap() -> Result<()> {
    facet_testhelpers::setup();

    let json = r#"{}"#;

    let _: HashMap<String, String> = from_str(json)?;
    Ok(())
}

#[test]
fn test_str_escaped() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Debug)]
    struct S {
        foo: String,
    }

    let json_ok = r#"{"foo":"\"\\abc"}"#;
    let ok: S = from_str(json_ok)?;
    assert_eq!(ok.foo, "\"\\abc");

    Ok(())
}
