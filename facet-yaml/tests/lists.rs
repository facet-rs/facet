use eyre::Result;
use facet::Facet;

#[derive(Debug, Facet, PartialEq)]
struct Person {
    name: String,
    age: u64,
}

#[test]
fn test_deserialize_primitive_list() -> Result<()> {
    let yaml = r#"
        - 1
        - 2
        - 3
        - 4
        - 5
    "#;

    let numbers: Vec<u64> = facet_yaml::from_str(yaml)?;
    assert_eq!(numbers, vec![1, 2, 3, 4, 5]);

    Ok(())
}

#[test]
fn test_deserialize_struct_list() -> Result<()> {
    let yaml = r#"
        - name: Alice
          age: 30
        - name: Bob
          age: 25
        - name: Charlie
          age: 35
    "#;

    let people: Vec<Person> = facet_yaml::from_str(yaml)?;
    assert_eq!(
        people,
        vec![
            Person {
                name: "Alice".to_string(),
                age: 30
            },
            Person {
                name: "Bob".to_string(),
                age: 25
            },
            Person {
                name: "Charlie".to_string(),
                age: 35
            }
        ]
    );

    Ok(())
}

#[test]
fn test_deserialize_empty_list() -> Result<()> {
    let yaml = r#"[]"#;

    let empty_list: Vec<u64> = facet_yaml::from_str(yaml)?;
    assert_eq!(empty_list, Vec::<u64>::new());

    Ok(())
}

#[test]
fn test_deserialize_nested_lists() -> Result<()> {
    let yaml = r#"
        - 
          - 1
          - 2
        -
          - 3
          - 4
    "#;

    let nested: Vec<Vec<u64>> = facet_yaml::from_str(yaml)?;
    assert_eq!(nested, vec![vec![1, 2], vec![3, 4]]);

    Ok(())
}
