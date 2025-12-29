use facet::Facet;
use facet_kdl_legacy as kdl;
use indoc::indoc;

// ============================================================================
// Option<T> behavior tests
// ============================================================================

/// Test that Option<T> fields WITHOUT #[facet(default)] require explicit values.
/// This follows facet conventions: Option<T> means "the value can be None",
/// not "the field can be omitted". Use #[facet(default)] to make a field optional.
#[test]
fn option_without_default_requires_value() {
    #[derive(Facet, Debug)]
    struct Config {
        #[facet(kdl::child)]
        server: Server,
    }

    #[derive(Facet, Debug)]
    struct Server {
        #[facet(kdl::argument)]
        host: String,
        #[facet(kdl::property)]
        port: Option<u16>, // No #[facet(default)] - requires explicit value!
    }

    // Missing port should fail
    let kdl = indoc! {r#"
        server "localhost"
    "#};

    let result: Result<Config, _> = facet_kdl_legacy::from_str(kdl);
    assert!(
        result.is_err(),
        "Option<T> without #[facet(default)] should require a value"
    );

    // Explicit #null should work for None
    let kdl_with_null = indoc! {r#"
        server "localhost" port=#null
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl_with_null).unwrap();
    assert_eq!(config.server.port, None);

    // Explicit value should work for Some
    let kdl_with_value = indoc! {r#"
        server "localhost" port=8080
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl_with_value).unwrap();
    assert_eq!(config.server.port, Some(8080));
}

/// Test that Option<T> fields WITH #[facet(default)] can be omitted.
#[test]
fn option_with_default_can_be_omitted() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        server: Server,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Server {
        #[facet(kdl::argument)]
        host: String,
        #[facet(kdl::property)]
        #[facet(default)]
        port: Option<u16>, // With #[facet(default)] - can be omitted
    }

    // Missing port should default to None
    let kdl = indoc! {r#"
        server "localhost"
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).unwrap();
    assert_eq!(config.server.port, None);

    // Explicit #null should also work
    let kdl_with_null = indoc! {r#"
        server "localhost" port=#null
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl_with_null).unwrap();
    assert_eq!(config.server.port, None);

    // Explicit value should work
    let kdl_with_value = indoc! {r#"
        server "localhost" port=8080
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl_with_value).unwrap();
    assert_eq!(config.server.port, Some(8080));
}

#[test]
fn hashmap_with_node_name_key() {
    use std::collections::HashMap;

    #[derive(Facet, Debug)]
    struct Config {
        #[facet(kdl::children)]
        settings: HashMap<String, String>,
    }

    let kdl = indoc! {r#"
        log_level "debug"
        timeout "30s"
        feature_flag "enabled"
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).unwrap();
    assert_eq!(config.settings.len(), 3);
    assert_eq!(config.settings.get("log_level"), Some(&"debug".to_string()));
    assert_eq!(config.settings.get("timeout"), Some(&"30s".to_string()));
    assert_eq!(
        config.settings.get("feature_flag"),
        Some(&"enabled".to_string())
    );
}

#[test]
fn btreemap_with_node_name_key() {
    use std::collections::BTreeMap;

    #[derive(Facet, Debug)]
    struct Config {
        #[facet(kdl::children)]
        settings: BTreeMap<String, i32>,
    }

    let kdl = indoc! {r#"
        port 8080
        timeout 30
        max_connections 100
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).unwrap();
    assert_eq!(config.settings.len(), 3);
    assert_eq!(config.settings.get("port"), Some(&8080));
    assert_eq!(config.settings.get("timeout"), Some(&30));
    assert_eq!(config.settings.get("max_connections"), Some(&100));

    // BTreeMap should iterate in sorted order
    let keys: Vec<_> = config.settings.keys().collect();
    assert_eq!(keys, vec!["max_connections", "port", "timeout"]);
}

#[test]
fn hashset_children() {
    use std::collections::HashSet;

    #[derive(Facet, Debug)]
    struct Config {
        #[facet(kdl::children)]
        tags: HashSet<Tag>,
    }

    #[derive(Facet, Debug, PartialEq, Eq, Hash)]
    struct Tag {
        #[facet(kdl::argument)]
        name: String,
    }

    let kdl = indoc! {r#"
        tag "rust"
        tag "kdl"
        tag "facet"
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).unwrap();
    assert_eq!(config.tags.len(), 3);

    // Check that all tags are present
    let names: HashSet<_> = config.tags.iter().map(|t| t.name.as_str()).collect();
    assert!(names.contains("rust"));
    assert!(names.contains("kdl"));
    assert!(names.contains("facet"));
}

#[test]
fn btreeset_children() {
    use std::collections::BTreeSet;

    #[derive(Facet, Debug, PartialEq, Eq, PartialOrd, Ord)]
    struct Priority {
        #[facet(kdl::argument)]
        level: u32,
    }

    #[derive(Facet, Debug)]
    struct Config {
        #[facet(kdl::children)]
        priorities: BTreeSet<Priority>,
    }

    let kdl = indoc! {r#"
        priority 3
        priority 1
        priority 2
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).unwrap();
    assert_eq!(config.priorities.len(), 3);

    // BTreeSet should iterate in sorted order
    let levels: Vec<_> = config.priorities.iter().map(|p| p.level).collect();
    assert_eq!(levels, vec![1, 2, 3]);
}

// ============================================================================
// Multiple kdl::children fields (issue #1096)
// ============================================================================

/// Test that multiple `#[facet(kdl::children)]` fields can coexist,
/// with nodes routed to the correct field based on node name matching
/// the singular form of the field name.
#[test]
fn multiple_children_fields_by_node_name() {
    #[derive(Facet, Debug)]
    struct Config {
        #[facet(kdl::children)]
        dependencies: Vec<Dependency>,

        #[facet(kdl::children)]
        samples: Vec<Sample>,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Dependency {
        #[facet(kdl::argument)]
        name: String,

        #[facet(kdl::property)]
        version: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Sample {
        #[facet(kdl::argument)]
        path: String,

        #[facet(kdl::property, default)]
        description: Option<String>,
    }

    // KDL with both dependency and sample nodes intermixed
    let kdl = indoc! {r#"
        dependency "serde" version="1.0"
        sample "test.txt" description="A test file"
        dependency "tokio" version="1.0"
        sample "example.txt"
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).unwrap();

    // Should have 2 dependencies
    assert_eq!(config.dependencies.len(), 2);
    assert_eq!(
        config.dependencies[0],
        Dependency {
            name: "serde".to_string(),
            version: "1.0".to_string()
        }
    );
    assert_eq!(
        config.dependencies[1],
        Dependency {
            name: "tokio".to_string(),
            version: "1.0".to_string()
        }
    );

    // Should have 2 samples
    assert_eq!(config.samples.len(), 2);
    assert_eq!(
        config.samples[0],
        Sample {
            path: "test.txt".to_string(),
            description: Some("A test file".to_string())
        }
    );
    assert_eq!(
        config.samples[1],
        Sample {
            path: "example.txt".to_string(),
            description: None
        }
    );
}

/// Test multiple children fields where only one type of node is present
#[test]
fn multiple_children_fields_partial() {
    #[derive(Facet, Debug)]
    struct Config {
        #[facet(kdl::children, default)]
        dependencies: Vec<Dependency>,

        #[facet(kdl::children, default)]
        samples: Vec<Sample>,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Dependency {
        #[facet(kdl::argument)]
        name: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Sample {
        #[facet(kdl::argument)]
        path: String,
    }

    // Only samples, no dependencies
    let kdl = indoc! {r#"
        sample "test.txt"
        sample "example.txt"
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).unwrap();

    assert_eq!(config.dependencies.len(), 0);
    assert_eq!(config.samples.len(), 2);
    assert_eq!(
        config.samples[0],
        Sample {
            path: "test.txt".to_string()
        }
    );
}

// Note: Multiple kdl::children fields with HashMap is not a well-supported use case.
// With HashMap, the node name becomes the map key, but with multiple fields,
// the node name is also used to route to the correct field.
// This creates a conflict where all nodes matching one field would have the same key.
// Use Vec for multiple children fields, or use a single HashMap field as a catch-all.

/// Test irregular plurals like children → child
#[test]
fn multiple_children_fields_irregular_plural() {
    #[derive(Facet, Debug)]
    struct Family {
        #[facet(kdl::children, default)]
        children: Vec<Child>,

        #[facet(kdl::children, default)]
        people: Vec<Person>,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Child {
        #[facet(kdl::argument)]
        name: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Person {
        #[facet(kdl::argument)]
        name: String,
    }

    let kdl = indoc! {r#"
        child "Alice"
        person "Bob"
        child "Charlie"
    "#};

    let family: Family = facet_kdl_legacy::from_str(kdl).unwrap();

    assert_eq!(family.children.len(), 2);
    assert_eq!(
        family.children[0],
        Child {
            name: "Alice".to_string()
        }
    );
    assert_eq!(
        family.children[1],
        Child {
            name: "Charlie".to_string()
        }
    );

    assert_eq!(family.people.len(), 1);
    assert_eq!(
        family.people[0],
        Person {
            name: "Bob".to_string()
        }
    );
}

/// Test that unknown nodes are skipped when there are multiple children fields
/// (unless deny_unknown_fields is set)
#[test]
fn multiple_children_fields_unknown_node_skipped() {
    #[derive(Facet, Debug)]
    struct Config {
        #[facet(kdl::children, default)]
        dependencies: Vec<Dependency>,

        #[facet(kdl::children, default)]
        samples: Vec<Sample>,
    }

    #[derive(Facet, Debug)]
    struct Dependency {
        #[facet(kdl::argument)]
        name: String,
    }

    #[derive(Facet, Debug)]
    struct Sample {
        #[facet(kdl::argument)]
        path: String,
    }

    // Unknown node type - should be skipped (default behavior)
    let kdl = indoc! {r#"
        unknown_node "test"
        sample "test.txt"
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).unwrap();
    assert_eq!(config.dependencies.len(), 0);
    assert_eq!(config.samples.len(), 1);
}

/// Test that unknown nodes error when deny_unknown_fields is set
/// and there are multiple children fields
#[test]
fn multiple_children_fields_deny_unknown() {
    #[derive(Facet, Debug)]
    #[facet(deny_unknown_fields)]
    struct Config {
        #[facet(kdl::children, default)]
        dependencies: Vec<Dependency>,

        #[facet(kdl::children, default)]
        samples: Vec<Sample>,
    }

    #[derive(Facet, Debug)]
    struct Dependency {
        #[facet(kdl::argument)]
        name: String,
    }

    #[derive(Facet, Debug)]
    struct Sample {
        #[facet(kdl::argument)]
        path: String,
    }

    // Unknown node type - should fail with deny_unknown_fields
    let kdl = indoc! {r#"
        unknown_node "test"
    "#};

    let result: Result<Config, _> = facet_kdl_legacy::from_str(kdl);
    assert!(
        result.is_err(),
        "Should fail on unknown node when deny_unknown_fields is set"
    );
}

/// Test custom node name override for kdl::children
/// This allows using node names that don't follow standard singular/plural patterns
#[test]
fn multiple_children_fields_custom_node_name() {
    #[derive(Facet, Debug)]
    struct Family {
        #[facet(kdl::children = "kiddo", default)]
        children: Vec<Child>,

        #[facet(kdl::children = "grownup", default)]
        adults: Vec<Adult>,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Child {
        #[facet(kdl::argument)]
        name: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Adult {
        #[facet(kdl::argument)]
        name: String,
    }

    let kdl = indoc! {r#"
        kiddo "Alice"
        grownup "Bob"
        kiddo "Charlie"
    "#};

    let family: Family = facet_kdl_legacy::from_str(kdl).unwrap();

    assert_eq!(family.children.len(), 2);
    assert_eq!(
        family.children[0],
        Child {
            name: "Alice".to_string()
        }
    );
    assert_eq!(
        family.children[1],
        Child {
            name: "Charlie".to_string()
        }
    );

    assert_eq!(family.adults.len(), 1);
    assert_eq!(
        family.adults[0],
        Adult {
            name: "Bob".to_string()
        }
    );
}

/// Test mixing custom node name with automatic singularization
#[test]
fn multiple_children_fields_mixed_node_name() {
    #[derive(Facet, Debug)]
    struct Config {
        // Uses automatic singularization: "dependency" -> "dependencies"
        #[facet(kdl::children, default)]
        dependencies: Vec<Dependency>,

        // Uses custom node name
        #[facet(kdl::children = "extra", default)]
        extras: Vec<Extra>,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Dependency {
        #[facet(kdl::argument)]
        name: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Extra {
        #[facet(kdl::argument)]
        value: String,
    }

    let kdl = indoc! {r#"
        dependency "serde"
        extra "debug-mode"
        dependency "tokio"
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).unwrap();

    assert_eq!(config.dependencies.len(), 2);
    assert_eq!(
        config.dependencies[0],
        Dependency {
            name: "serde".to_string()
        }
    );
    assert_eq!(
        config.dependencies[1],
        Dependency {
            name: "tokio".to_string()
        }
    );

    assert_eq!(config.extras.len(), 1);
    assert_eq!(
        config.extras[0],
        Extra {
            value: "debug-mode".to_string()
        }
    );
}

/// Test that custom node names round-trip correctly through serialization
#[test]
fn custom_node_name_round_trip() {
    #[derive(Facet, Debug, PartialEq)]
    struct Family {
        #[facet(kdl::children = "kiddo", default)]
        children: Vec<Child>,

        #[facet(kdl::children = "grownup", default)]
        adults: Vec<Adult>,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Child {
        #[facet(kdl::argument)]
        name: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Adult {
        #[facet(kdl::argument)]
        name: String,
    }

    let original = Family {
        children: vec![
            Child {
                name: "Alice".to_string(),
            },
            Child {
                name: "Charlie".to_string(),
            },
        ],
        adults: vec![Adult {
            name: "Bob".to_string(),
        }],
    };

    // Serialize
    let kdl_string = facet_kdl_legacy::to_string(&original).unwrap();

    // Verify it uses the custom node names
    assert!(
        kdl_string.contains("kiddo"),
        "Expected 'kiddo' nodes, got:\n{kdl_string}"
    );
    assert!(
        kdl_string.contains("grownup"),
        "Expected 'grownup' nodes, got:\n{kdl_string}"
    );

    // Round-trip: deserialize back
    let deserialized: Family = facet_kdl_legacy::from_str(&kdl_string).unwrap();

    assert_eq!(original, deserialized);
}
