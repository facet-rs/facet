use facet::Facet;
use facet_json::from_str;
use facet_testhelpers::test;

#[test]
fn json_read_empty_vec() {
    let json = r#"[]"#;

    let v: Vec<i32> = from_str(json).unwrap();
    assert_eq!(v, vec![]);
}

#[test]
fn json_read_vec() {
    let json = r#"[1, 2, 3, 4, 5]"#;

    let v: Vec<u64> = from_str(json).unwrap();
    assert_eq!(v, vec![1, 2, 3, 4, 5]);
}

#[test]
fn test_two_empty_vecs() {
    #[derive(Facet, Clone, Default)]
    pub struct RevisionConfig {
        pub one: Vec<String>,
        pub two: Vec<String>,
    }

    let markup = r#"
    {
      "one": [],
      "two": []
    }
    "#;

    let config: RevisionConfig = from_str(markup).unwrap();
    assert!(config.one.is_empty());
    assert!(config.two.is_empty());
}

#[test]
fn test_one_empty_one_nonempty_vec() {
    #[derive(Facet, Clone, Default)]
    pub struct RevisionConfig {
        pub one: Vec<String>,
        pub two: Vec<String>,
    }

    let markup = r#"
    {
      "one": [],
      "two": ["a", "b", "c"]
    }
    "#;

    let config: RevisionConfig = from_str(markup).unwrap();
    assert!(config.one.is_empty());
    assert_eq!(config.two, vec!["a", "b", "c"]);
}

#[test]
fn test_one_nonempty_one_empty_vec() {
    #[derive(Facet, Clone, Default)]
    pub struct RevisionConfig {
        pub one: Vec<String>,
        pub two: Vec<String>,
    }

    let markup = r#"
    {
      "one": ["x", "y"],
      "two": []
    }
    "#;

    let config: RevisionConfig = from_str(markup).unwrap();
    assert_eq!(config.one, vec!["x", "y"]);
    assert!(config.two.is_empty());
}

#[test]
fn test_nested_arrays() {
    #[derive(Facet, Clone, Default)]
    pub struct NestedArrays {
        pub matrix: Vec<Vec<u64>>,
    }

    let markup = r#"
    {
      "matrix": [
        [1, 2, 3],
        [],
        [4, 5]
      ]
    }
    "#;

    let nested: NestedArrays = from_str(markup).unwrap();
    assert_eq!(nested.matrix.len(), 3);
    assert_eq!(nested.matrix[0], vec![1, 2, 3]);
    assert_eq!(nested.matrix[1], vec![]);
    assert_eq!(nested.matrix[2], vec![4, 5]);
}

#[test]
fn test_deserialize_list() {
    let result: Vec<i32> = from_str(r#"[1,3]"#).unwrap();
    assert_eq!(result[0], 1);
    assert_eq!(result[1], 3);
}

#[test]
fn test_vec_of_structs_fine() {
    #[derive(Facet)]
    struct FooBar {
        foo: u64,
        bar: String,
    }

    let payload = r#"[
    {
        "foo": 32,
        "bar": "hello"
    },
    {
        "foo": 64,
        "bar": "world"
    }
]"#;

    let foos: Vec<FooBar> = from_str(payload)?;
    assert_eq!(foos.len(), 2);
    assert_eq!(foos[0].foo, 32);
    assert_eq!(foos[0].bar, "hello");
    assert_eq!(foos[1].foo, 64);
    assert_eq!(foos[1].bar, "world");
}

#[test]
fn test_vec_of_structs_missing_field() {
    #[derive(Facet, Debug)]
    struct FooBar {
        foo: u64,
        bar: String,
    }

    let payload = r#"[
    {
        "foo": 32,
        "bar": "hello"
    },
    {
        "foo": 64
    }
]"#;

    let result = from_str::<Vec<FooBar>>(payload);
    #[cfg(not(miri))]
    insta::assert_debug_snapshot!(result);
}
