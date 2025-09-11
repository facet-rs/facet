use facet::Facet;
use facet_testhelpers::test;

#[test]
#[ignore]
fn test_struct_level_default() {
    #[derive(Facet, Default, Debug, PartialEq)]
    #[facet(default)]
    struct DefaultStruct {
        foo: i32,
        bar: String,
    }

    // Only set foo, leave bar missing - should use Default for String
    let yaml = r#"
        foo: 123
    "#;

    let s: DefaultStruct = facet_yaml::from_str(yaml).unwrap();
    assert_eq!(s.foo, 123, "Expected foo to be 123, got {}", s.foo);
    assert!(
        s.bar.is_empty(),
        "Expected bar to be empty string, got {:?}",
        s.bar
    );
}

#[test]
fn test_field_level_default_no_function() {
    #[derive(Facet, Debug, PartialEq)]
    struct FieldDefault {
        foo: i32,
        #[facet(default)]
        bar: String,
    }

    // Only set foo, leave bar missing - should use Default for String
    let yaml = r#"
        foo: 789
    "#;

    let s: FieldDefault = facet_yaml::from_str(yaml).unwrap();
    assert_eq!(s.foo, 789, "Expected foo to be 789, got {}", s.foo);
    assert_eq!(
        s.bar, "",
        "Expected bar to be empty string, got {:?}",
        s.bar
    );
}

#[test]
fn test_field_level_default_function() {
    fn default_number() -> i32 {
        12345
    }

    #[derive(Facet, Debug, PartialEq)]
    struct FieldDefaultFn {
        #[facet(default = default_number())]
        foo: i32,
        bar: String,
    }

    // Only set bar, leave foo missing - should use default_number()
    let yaml = r#"
        bar: hello
    "#;

    let s: FieldDefaultFn = facet_yaml::from_str(yaml).unwrap();
    assert_eq!(s.foo, 12345, "Expected foo to be 12345, got {}", s.foo);
    assert_eq!(s.bar, "hello", "Expected bar to be 'hello', got {}", s.bar);
}

#[test]
fn test_nested_struct_with_defaults() {
    #[derive(Facet, Debug, PartialEq)]
    struct Root {
        value: i32,
        #[facet(default)]
        nested: Nested,
    }

    #[derive(Facet, Debug, PartialEq, Default)]
    struct Nested {
        #[facet(default = 42)]
        answer: i32,
        #[facet(default)]
        name: String,
    }

    // When nested isn't there at all but has #[facet(default)]
    // it should initialize nested with Default::default()
    // but the nested answer field default doesn't apply unless it's in the YAML
    let yaml = r#"
        value: 100
    "#;

    let s: Root = facet_yaml::from_str(yaml).unwrap();
    assert_eq!(s.value, 100);
    println!("Got nested.answer = {}, expected 0", s.nested.answer);
    assert_eq!(
        s.nested.answer, 0,
        "Expected 0 for answer field when nested uses Default"
    );
    assert_eq!(s.nested.name, "", "Expected empty string for name field");

    // Set the root value and partially set nested struct
    // When nested is in the YAML, the field-level defaults should apply to missing fields
    let yaml2 = r#"
        value: 100
        nested:
            name: test
    "#;

    let s2: Root = facet_yaml::from_str(yaml2).unwrap();
    assert_eq!(s2.value, 100);
    assert_eq!(
        s2.nested.answer, 42,
        "Expected 42 when nested struct provided but answer is unset"
    );
    assert_eq!(
        s2.nested.name, "test",
        "Expected 'test' for explicitly set name field"
    );
}

#[test]
fn test_default_with_complex_expression() {
    const DEFAULT_PORT: u16 = 8080;

    fn default_timeout() -> u64 {
        30
    }

    #[derive(Facet, Debug, PartialEq)]
    struct ServerConfig {
        host: String,
        #[facet(default = DEFAULT_PORT)]
        port: u16,
        #[facet(default = default_timeout())]
        timeout_seconds: u64,
        #[facet(default = vec!["user".to_string(), "admin".to_string()])]
        default_roles: Vec<String>,
    }

    // Only set host
    let yaml = r#"
        host: localhost
    "#;

    let config: ServerConfig = facet_yaml::from_str(yaml).unwrap();
    assert_eq!(config.host, "localhost");
    assert_eq!(config.port, 8080);
    assert_eq!(config.timeout_seconds, 30);
    assert_eq!(
        config.default_roles,
        vec!["user".to_string(), "admin".to_string()]
    );

    // Override defaults
    let yaml2 = r#"
        host: example.com
        port: 9000
        timeout_seconds: 60
        default_roles:
          - guest
    "#;

    let config2: ServerConfig = facet_yaml::from_str(yaml2).unwrap();
    assert_eq!(config2.host, "example.com");
    assert_eq!(config2.port, 9000);
    assert_eq!(config2.timeout_seconds, 60);
    assert_eq!(config2.default_roles, vec!["guest".to_string()]);
}
