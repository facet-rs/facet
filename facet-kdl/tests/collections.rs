use facet::Facet;
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

    let result: Result<Config, _> = facet_kdl::from_str(kdl);
    assert!(
        result.is_err(),
        "Option<T> without #[facet(default)] should require a value"
    );

    // Explicit #null should work for None
    let kdl_with_null = indoc! {r#"
        server "localhost" port=#null
    "#};

    let config: Config = facet_kdl::from_str(kdl_with_null).unwrap();
    assert_eq!(config.server.port, None);

    // Explicit value should work for Some
    let kdl_with_value = indoc! {r#"
        server "localhost" port=8080
    "#};

    let config: Config = facet_kdl::from_str(kdl_with_value).unwrap();
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

    let config: Config = facet_kdl::from_str(kdl).unwrap();
    assert_eq!(config.server.port, None);

    // Explicit #null should also work
    let kdl_with_null = indoc! {r#"
        server "localhost" port=#null
    "#};

    let config: Config = facet_kdl::from_str(kdl_with_null).unwrap();
    assert_eq!(config.server.port, None);

    // Explicit value should work
    let kdl_with_value = indoc! {r#"
        server "localhost" port=8080
    "#};

    let config: Config = facet_kdl::from_str(kdl_with_value).unwrap();
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

    let config: Config = facet_kdl::from_str(kdl).unwrap();
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

    let config: Config = facet_kdl::from_str(kdl).unwrap();
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

    let config: Config = facet_kdl::from_str(kdl).unwrap();
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

    let config: Config = facet_kdl::from_str(kdl).unwrap();
    assert_eq!(config.priorities.len(), 3);

    // BTreeSet should iterate in sorted order
    let levels: Vec<_> = config.priorities.iter().map(|p| p.level).collect();
    assert_eq!(levels, vec![1, 2, 3]);
}
