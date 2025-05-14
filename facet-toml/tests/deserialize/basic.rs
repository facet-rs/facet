use facet::Facet;
use facet_testhelpers::test;

#[derive(Debug, Facet, PartialEq)]
struct Person {
    name: String,
    age: u64,
}

#[test]
fn test_deserialize_person() {
    let toml = r#"
            name = "Alice"
            age = 30
        "#;

    let person: Person = facet_toml::from_str(toml)?;
    assert_eq!(
        person,
        Person {
            name: "Alice".to_string(),
            age: 30
        }
    );
}
