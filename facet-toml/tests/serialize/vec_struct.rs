//! Test for Vec<Struct> serialization

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
fn test_serialize_vec_struct() {
    let root = Root {
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
    };

    let toml = facet_toml::to_string(&root)?;

    // This is what we get currently (inline array)
    println!("Current output:\n{}", toml);

    // This is what we want (array of tables)
    let expected =
        "[[people]]\nname = \"Alice\"\nage = 30\n\n[[people]]\nname = \"Bob\"\nage = 25\n";

    // This test will fail with the current implementation
    assert_eq!(toml, expected);
}
