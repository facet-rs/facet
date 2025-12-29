use facet::Facet;
use facet_testhelpers::test;

#[derive(Debug, Facet, PartialEq)]
struct Person {
    name: String,
    age: u64,
}

#[test]
fn test_deserialize_person() {
    let yaml = r#"
            name: Alice
            age: 30
        "#;

    let person: Person = facet_yaml_legacy::from_str(yaml).unwrap();
    assert_eq!(
        person,
        Person {
            name: "Alice".to_string(),
            age: 30
        }
    );
}

/// Tests the use case from issue #1074 - parsing a config file from a temporary buffer.
/// This demonstrates that `from_str` works with owned types without requiring the input
/// to outlive the result.
#[derive(Debug, Facet, PartialEq)]
struct Config {
    name: String,
    port: u16,
}

fn load_config_from_temp_buffer() -> Config {
    // Simulate reading a config file into a temporary buffer (e.g., HTTP request body)
    let yaml_content = String::from("name: myapp\nport: 8080");

    // The key feature: yaml_content is dropped after this function returns,
    // but the Config is fully owned and can outlive the input buffer.
    // This would NOT work with borrowed deserialization.
    facet_yaml_legacy::from_str(&yaml_content).unwrap()
}

#[test]
fn test_owned_deserialization_from_temp_buffer() {
    // This tests issue #1074 - the ability to parse owned values from temporary buffers
    let config = load_config_from_temp_buffer();
    assert_eq!(config.name, "myapp");
    assert_eq!(config.port, 8080);
}

/// Tests that both from_str (owned) and from_str_borrowed work correctly.
#[test]
fn test_from_str_vs_from_str_borrowed() {
    let yaml = "name: test\nport: 3000";

    // Owned deserialization - input doesn't need to outlive result
    let config_owned: Config = facet_yaml_legacy::from_str(yaml).unwrap();

    // Borrowed deserialization - input must outlive result (but we can still use it here)
    let config_borrowed: Config = facet_yaml_legacy::from_str_borrowed(yaml).unwrap();

    assert_eq!(config_owned, config_borrowed);
}
