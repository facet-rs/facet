use super::*;
use facet_styx::RenderError;
use facet_testhelpers::test;

/// Test that Spanned<String> works as a map key with facet-styx.
#[test]
fn spanned_string_as_map_key() {
    #[derive(Debug, Facet)]
    struct TestMap {
        #[facet(flatten)]
        items: IndexMap<Meta<String>, String>,
    }

    let source = r#"{foo bar, baz qux}"#;
    let result: Result<TestMap, _> = facet_styx::from_str(source);

    match result {
        Ok(map) => {
            assert_eq!(map.items.len(), 2);
            let keys: Vec<_> = map.items.keys().map(|k| k.value.as_str()).collect();
            assert!(keys.contains(&"foo"));
            assert!(keys.contains(&"baz"));
        }
        Err(e) => {
            panic!("Failed to parse: {}", e.render("<test>", source));
        }
    }
}

/// Test that Where clause parses correctly with Spanned keys.
#[test]
fn where_clause_with_spanned_keys() {
    let source = r#"{deleted_at @null}"#;
    let result: Result<Where, _> = facet_styx::from_str(source);

    match result {
        Ok(where_clause) => {
            assert_eq!(where_clause.filters.len(), 1);
            let key = where_clause.filters.keys().next().unwrap();
            assert_eq!(key.value.as_str(), "deleted_at");
        }
        Err(e) => {
            panic!("Failed to parse: {}", e.render("<test>", source));
        }
    }
}

/// Test that FilterValue::EqBare works with Meta<String>.
#[test]
fn filter_value_eq() {
    let source = r#"{id $id}"#;
    let result: Result<Where, _> = facet_styx::from_str(source);

    match result {
        Ok(where_clause) => {
            assert_eq!(where_clause.filters.len(), 1);
            let (key, value) = where_clause.filters.iter().next().unwrap();
            assert_eq!(key.value.as_str(), "id");
            match value {
                FilterValue::EqBare(Some(meta)) => {
                    assert_eq!(meta.as_str(), "$id");
                    // Verify span is captured (offset 4, len 3 for "$id")
                    assert_eq!(meta.span.offset, 4);
                    assert_eq!(meta.span.len, 3);
                }
                _ => panic!("Expected EqBare variant, got {:?}", value),
            }
        }
        Err(e) => {
            panic!("Failed to parse: {}", e.render("<test>", source));
        }
    }
}

/// Test that FilterValue::EqBare works with shorthand (no value).
#[test]
fn filter_value_eq_shorthand() {
    let source = r#"{id}"#;
    let result: Result<Where, _> = facet_styx::from_str(source);

    match result {
        Ok(where_clause) => {
            assert_eq!(where_clause.filters.len(), 1);
            let (key, value) = where_clause.filters.iter().next().unwrap();
            assert_eq!(key.value.as_str(), "id");
            match value {
                FilterValue::EqBare(None) => {
                    // Success - shorthand syntax where {id} means {id $id}
                }
                FilterValue::EqBare(Some(meta)) => {
                    panic!(
                        "Expected EqBare(None) for shorthand, got EqBare(Some({}))",
                        meta.as_str()
                    );
                }
                _ => panic!("Expected EqBare variant, got {:?}", value),
            }
        }
        Err(e) => {
            panic!("Failed to parse: {}", e.render("<test>", source));
        }
    }
}

#[test]
fn test_fixtures_queries1() {
    let source = include_str!("./fixtures/queries1.styx");
    let result: Result<QueryFile, _> = facet_styx::from_str(source);

    match result {
        Ok(_where_clause) => {
            // cool
        }
        Err(e) => {
            panic!("Failed to parse: {}", e.render("<test>", source));
        }
    }
}

#[test]
fn test_fixtures_queries1b() {
    let source = include_str!("./fixtures/queries1b.styx");
    let result: Result<QueryFile, _> = facet_styx::from_str(source);

    match result {
        Ok(_where_clause) => {
            panic!("Should NOT parse queries1b, it has triple-quotes which is not styx syntax");
        }
        Err(e) => {
            eprintln!("Error as expected: {}", e.render("<test>", source));
        }
    }
}

#[test]
fn test_fixtures_queries2() {
    let source = include_str!("./fixtures/queries2.styx");
    let result: Result<QueryFile, _> = facet_styx::from_str(source);

    match result {
        Ok(_where_clause) => {
            // cool
        }
        Err(e) => {
            panic!("Failed to parse: {}", e.render("<test>", source));
        }
    }
}

/// Test that the QueryFile schema can be generated and parsed back (roundtrip).
///
/// This validates that facet-styx schema generation works correctly for types
/// with `#[facet(other)]` variants like FilterValue::EqBare and ValueExpr::Other.
#[test]
fn test_query_file_schema_roundtrip() {
    use facet_styx::SchemaFile;

    // Generate the schema from the QueryFile type
    let schema_str = facet_styx::schema_from_type::<QueryFile>();
    eprintln!("Generated QueryFile schema:\n{schema_str}");

    // Parse it back into a SchemaFile - this validates the schema is well-formed
    let schema_file: SchemaFile = facet_styx::from_str(&schema_str)
        .expect("Generated QueryFile schema should parse back into SchemaFile");

    // Verify the schema has expected structure
    assert!(
        schema_file.schema.contains_key(&None),
        "Schema should have root definition at @"
    );

    // Check key type definitions exist
    let expected_types = [
        "Select",
        "Insert",
        "Update",
        "Delete",
        "Upsert",
        "Where",
        "FilterValue",
        "ValueExpr",
        "Relation",
        "Params",
        "ParamType",
    ];

    for type_name in expected_types {
        assert!(
            schema_file
                .schema
                .contains_key(&Some(type_name.to_string())),
            "Schema should contain {type_name} type definition. Available types: {:?}",
            schema_file
                .schema
                .keys()
                .filter_map(|k| k.as_ref())
                .collect::<Vec<_>>()
        );
    }

    // Verify FilterValue is @any (because it has #[facet(other)] variant)
    let filter_value = schema_file
        .schema
        .get(&Some("FilterValue".to_string()))
        .expect("FilterValue should exist");
    assert!(
        matches!(filter_value, facet_styx::Schema::Any),
        "FilterValue should be @any due to #[facet(other)] variant, got {filter_value:?}"
    );

    // Verify ValueExpr is @any (because it has #[facet(other)] variant)
    let value_expr = schema_file
        .schema
        .get(&Some("ValueExpr".to_string()))
        .expect("ValueExpr should exist");
    assert!(
        matches!(value_expr, facet_styx::Schema::Any),
        "ValueExpr should be @any due to #[facet(other)] variant, got {value_expr:?}"
    );
}
