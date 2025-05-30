use facet::Facet;

#[derive(Debug, PartialEq, Clone, Facet)]
struct SimpleOption {
    value: Option<String>,
}

#[test]
fn test_simple_option_some() {
    let toml_str = r#"value = "hello""#;

    let result: Result<SimpleOption, _> = facet_toml::from_str(toml_str);
    if let Err(e) = &result {
        eprintln!("Error: {}", e);
    }
    assert!(result.is_ok());
    assert_eq!(result.unwrap().value, Some("hello".to_string()));
}

#[test]
fn test_simple_option_none() {
    let toml_str = r#""#;

    let result: Result<SimpleOption, _> = facet_toml::from_str(toml_str);
    if let Err(e) = &result {
        eprintln!("Error: {}", e);
    }
    assert!(result.is_ok());
    assert_eq!(result.unwrap().value, None);
}

#[test]
fn test_multiple_options() {
    #[derive(Debug, PartialEq, Clone, Facet)]
    struct MultipleOptions {
        opt1: Option<String>,
        opt2: Option<i32>,
        opt3: Option<String>,
    }

    let toml_str = r#"
opt1 = "hello"
opt3 = "world"
"#;

    let result: Result<MultipleOptions, _> = facet_toml::from_str(toml_str);
    if let Err(e) = &result {
        eprintln!("Error: {}", e);
    }
    assert!(result.is_ok());
    let value = result.unwrap();
    assert_eq!(value.opt1, Some("hello".to_string()));
    assert_eq!(value.opt2, None);
    assert_eq!(value.opt3, Some("world".to_string()));
}
