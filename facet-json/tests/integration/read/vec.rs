use facet::Facet;
use facet_json::from_str;

#[test]
fn json_read_empty_vec() {
    facet_testhelpers::setup();

    let json = r#"[]"#;

    let v: Vec<i32> = match from_str(json) {
        Ok(v) => v,
        Err(e) => panic!("Error deserializing JSON: {}", e),
    };
    assert_eq!(v, vec![]);
}

#[test]
fn json_read_vec() {
    facet_testhelpers::setup();

    let json = r#"[1, 2, 3, 4, 5]"#;

    let v: Vec<u64> = match from_str(json) {
        Ok(v) => v,
        Err(e) => panic!("Error deserializing JSON: {}", e),
    };
    assert_eq!(v, vec![1, 2, 3, 4, 5]);
}

#[test]
fn test_two_empty_vecs() {
    facet_testhelpers::setup();

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

    let config: RevisionConfig = match from_str(markup) {
        Ok(cfg) => cfg,
        Err(e) => panic!("Failed to parse RevisionConfig: {}", e),
    };
    assert!(config.one.is_empty());
    assert!(config.two.is_empty());
}

#[test]
fn test_one_empty_one_nonempty_vec() {
    facet_testhelpers::setup();

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

    let config: RevisionConfig = match from_str(markup) {
        Ok(cfg) => cfg,
        Err(e) => panic!("Failed to parse RevisionConfig: {}", e),
    };
    assert!(config.one.is_empty());
    assert_eq!(config.two, vec!["a", "b", "c"]);
}

#[test]
fn test_one_nonempty_one_empty_vec() {
    facet_testhelpers::setup();

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

    let config: RevisionConfig = match from_str(markup) {
        Ok(cfg) => cfg,
        Err(e) => panic!("Failed to parse RevisionConfig: {}", e),
    };
    assert_eq!(config.one, vec!["x", "y"]);
    assert!(config.two.is_empty());
}

#[test]
fn test_nested_arrays() {
    facet_testhelpers::setup();

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

    let nested: NestedArrays = match from_str(markup) {
        Ok(cfg) => cfg,
        Err(e) => panic!("Failed to parse NestedArrays: {}", e),
    };
    assert_eq!(nested.matrix.len(), 3);
    assert_eq!(nested.matrix[0], vec![1, 2, 3]);
    assert_eq!(nested.matrix[1], vec![]);
    assert_eq!(nested.matrix[2], vec![4, 5]);
}

#[test]
#[ignore]
fn test_deserialize_tuple() {
    let result: Result<(&str, i32), _> = from_str(r#"["aaa",3]"#);
    let ok = result.unwrap();
    assert_eq!(ok.0, "aaa");
    assert_eq!(ok.1, 3);

    #[derive(facet::Facet)]
    struct TestTuple(i32, String, bool);
    let result: Result<TestTuple, _> = from_str(r#"[3,"aaa",true]"#);
    let ok = result.unwrap();
    assert_eq!(ok.0, 3);
    assert_eq!(ok.1, "aaa");
    assert!(ok.2);
}

#[test]
fn test_deserialize_list() {
    let result: Result<Vec<i32>, _> = from_str(r#"[1,3]"#);
    let ok = result.unwrap();
    assert_eq!(ok[0], 1);
    assert_eq!(ok[1], 3);
}
