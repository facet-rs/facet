//! Test for Vec<Struct> deserialization

use facet::Facet;
use facet_testhelpers::test;

#[derive(Debug, Facet, PartialEq)]
struct Person {
    name: String,
    age: u64,
}

#[derive(Debug, Facet, PartialEq)]
struct Root {
    people: Vec<Person>,
}

#[test]
fn test_deserialize_vec_struct() {
    assert_eq!(
        facet_toml::from_str::<Root>(
            r#"
            [[people]]
            name = "Alice"
            age = 30

            [[people]]
            name = "Bob"
            age = 25
            "#
        )
        .unwrap(),
        Root {
            people: vec![
                Person {
                    name: "Alice".to_string(),
                    age: 30,
                },
                Person {
                    name: "Bob".to_string(),
                    age: 25,
                },
            ],
        }
    );
}
