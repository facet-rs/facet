use facet::Facet;
use facet_testhelpers::test;

#[derive(Debug, Facet, PartialEq, Clone)]
#[facet(transparent)]
struct MyString(String);

#[test]
fn test_transparent_string() {
    let yaml = r#""Hello, world!""#;

    let my_string: MyString = facet_yaml::from_str(yaml).unwrap();
    assert_eq!(my_string.0, "Hello, world!".to_string());
}

#[test]
fn test_transparent_string_vec() {
    let yaml = r#"
        - "Hello"
        - "World"
        - "!"
    "#;

    let strings: Vec<MyString> = facet_yaml::from_str(yaml).unwrap();
    assert_eq!(
        strings,
        vec![
            MyString("Hello".to_string()),
            MyString("World".to_string()),
            MyString("!".to_string())
        ]
    );
}

#[test]
fn test_transparent_in_struct() {
    #[derive(Debug, Facet, PartialEq)]
    struct Message {
        content: MyString,
        tags: Vec<MyString>,
    }

    let yaml = r#"
        content: "Important message"
        tags:
          - "urgent"
          - "notification"
    "#;

    let message: Message = facet_yaml::from_str(yaml).unwrap();
    assert_eq!(
        message,
        Message {
            content: MyString("Important message".to_string()),
            tags: vec![
                MyString("urgent".to_string()),
                MyString("notification".to_string())
            ]
        }
    );
}
