use facet_value::Value;

#[test]
fn test_array_subtables() {
    let toml = r#"[[arr]]
[arr.subtab]
val=1

[[arr]]
[arr.subtab]
val=2"#;

    let result: Result<Value, _> = facet_toml_legacy::from_str(toml);
    println!("{result:?}");
}
