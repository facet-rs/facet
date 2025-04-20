use std::collections::HashMap;

use facet::Facet;
use facet_json::from_str;

#[test]
fn json_read_empty_object_for_struct() -> eyre::Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet)]
    struct TestStruct {}
    let json = r#"{}"#;

    let _: TestStruct = from_str(json)?;
    Ok(())
}

#[ignore]
#[test]
fn json_read_empty_object_for_hashmap() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let json = r#"{}"#;

    let _: HashMap<String, String> = from_str(json)?;
    Ok(())
}

#[test]
fn test_str_escaped() {
    facet_testhelpers::setup();

    #[derive(Facet, Debug)]
    struct S {
        foo: String,
    }

    let json_ok = r#"{"foo":"\"\\abc"}"#;
    let result_ok: Result<S, _> = from_str(json_ok);
    assert_eq!(&result_ok.unwrap().foo, "\"\\abc");
}
