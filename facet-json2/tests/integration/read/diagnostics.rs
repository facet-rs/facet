use facet_json2::from_str;

#[test]
fn test_debug_format_for_errors() {
    facet_testhelpers::setup();

    let result = from_str::<i32>("x");
    let err = result.unwrap_err();

    let debug_str = format!("{:?}", err);
    assert!(!debug_str.is_empty());
}

#[test]
fn test_with_rich_diagnostics() {
    facet_testhelpers::setup();

    let result = from_str::<i32>("x");
    let err = result.unwrap_err();

    // This should trigger the rich diagnostics display code
    let display_str = format!("{}", err);

    insta::assert_snapshot!(display_str);
}
