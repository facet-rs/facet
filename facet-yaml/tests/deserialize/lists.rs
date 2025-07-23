use facet::Facet;
use facet_testhelpers::test;
use std::sync::Arc;

#[derive(Debug, Facet, PartialEq)]
struct Person {
    name: String,
    age: u64,
}

#[test]
fn test_deserialize_primitive_list() {
    let yaml = r#"
        - 1
        - 2
        - 3
        - 4
        - 5
    "#;

    let numbers: Vec<u64> = facet_yaml::from_str(yaml).unwrap();
    assert_eq!(numbers, vec![1, 2, 3, 4, 5]);
}

#[test]
fn test_deserialize_struct_list() {
    let yaml = r#"
        - name: Alice
          age: 30
        - name: Bob
          age: 25
        - name: Charlie
          age: 35
    "#;

    let people: Vec<Person> = facet_yaml::from_str(yaml).unwrap();
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
}

#[test]
fn test_deserialize_empty_list() {
    let yaml = r#"[]"#;

    let empty_list: Vec<u64> = facet_yaml::from_str(yaml).unwrap();
    assert_eq!(empty_list, Vec::<u64>::new());
}

#[test]
fn test_deserialize_nested_lists() {
    let yaml = r#"
        -
          - 1
          - 2
        -
          - 3
          - 4
    "#;

    let nested: Vec<Vec<u64>> = facet_yaml::from_str(yaml).unwrap();
    assert_eq!(nested, vec![vec![1, 2], vec![3, 4]]);
}

#[test]
fn test_deserialize_arc_slice_i32() {
    let yaml = r#"[1, 2, 3, 4, 5]"#;

    let arc_slice: Arc<[i32]> = facet_yaml::from_str(yaml).unwrap();
    assert_eq!(arc_slice.as_ref(), &[1, 2, 3, 4, 5]);
}

#[test]
fn test_deserialize_arc_slice_string() {
    let yaml = r#"["hello", "world", "test"]"#;

    let arc_slice: Arc<[String]> = facet_yaml::from_str(yaml).unwrap();
    assert_eq!(arc_slice.len(), 3);
    assert_eq!(&arc_slice[0], "hello");
    assert_eq!(&arc_slice[1], "world");
    assert_eq!(&arc_slice[2], "test");
}

#[test]
fn test_deserialize_arc_slice_empty() {
    let yaml = r#"[]"#;

    let arc_slice: Arc<[i32]> = facet_yaml::from_str(yaml).unwrap();
    assert!(arc_slice.is_empty());
}

#[test]
fn test_deserialize_arc_slice_struct() {
    let yaml = r#"
        - name: Alice
          age: 30
        - name: Bob
          age: 25
    "#;

    let people: Arc<[Person]> = facet_yaml::from_str(yaml).unwrap();
    assert_eq!(people.len(), 2);
    assert_eq!(people[0].name, "Alice");
    assert_eq!(people[0].age, 30);
    assert_eq!(people[1].name, "Bob");
    assert_eq!(people[1].age, 25);
}
