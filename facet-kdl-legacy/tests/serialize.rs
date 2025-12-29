use facet::Facet;
use facet_kdl_legacy as kdl;

// ============================================================================
// Serialization tests
// ============================================================================

/// Test basic serialization of a simple struct with child nodes.
#[test]
fn serialize_basic_struct() {
    #[derive(Facet, PartialEq, Debug)]
    struct Config {
        #[facet(kdl::child)]
        server: Server,
    }

    #[derive(Facet, PartialEq, Debug)]
    struct Server {
        #[facet(kdl::argument)]
        host: String,
        #[facet(kdl::property)]
        port: u16,
    }

    let config = Config {
        server: Server {
            host: "localhost".to_string(),
            port: 8080,
        },
    };

    let kdl = facet_kdl_legacy::to_string(&config).unwrap();
    assert!(kdl.contains("server"));
    assert!(kdl.contains("\"localhost\""));
    assert!(kdl.contains("port=8080"));

    // Round-trip test
    let parsed: Config = facet_kdl_legacy::from_str(&kdl).unwrap();
    assert_eq!(parsed, config);
}

/// Test serialization with multiple arguments.
#[test]
fn serialize_multiple_arguments() {
    #[derive(Facet, PartialEq, Debug)]
    struct Doc {
        #[facet(kdl::child)]
        matrix: Matrix,
    }

    #[derive(Facet, PartialEq, Debug)]
    struct Matrix {
        #[facet(kdl::arguments)]
        values: Vec<u8>,
    }

    let doc = Doc {
        matrix: Matrix {
            values: vec![1, 2, 3, 4, 5],
        },
    };

    let kdl = facet_kdl_legacy::to_string(&doc).unwrap();
    assert!(kdl.contains("matrix"));
    assert!(kdl.contains("1"));
    assert!(kdl.contains("5"));

    // Round-trip test
    let parsed: Doc = facet_kdl_legacy::from_str(&kdl).unwrap();
    assert_eq!(parsed, doc);
}

/// Test serialization with Optional fields - should skip None values.
#[test]
fn serialize_optional_fields() {
    #[derive(Facet, PartialEq, Debug)]
    struct Config {
        #[facet(kdl::child)]
        server: Server,
    }

    #[derive(Facet, PartialEq, Debug)]
    struct Server {
        #[facet(kdl::argument)]
        host: String,
        #[facet(kdl::property)]
        port: Option<u16>,
        #[facet(kdl::property)]
        timeout: Option<u32>,
    }

    // With Some values
    let config_with_values = Config {
        server: Server {
            host: "localhost".to_string(),
            port: Some(8080),
            timeout: None,
        },
    };

    let kdl = facet_kdl_legacy::to_string(&config_with_values).unwrap();
    assert!(kdl.contains("port=8080"));
    assert!(!kdl.contains("timeout")); // None should be skipped
}

/// Test serialization with enum child nodes.
#[test]
fn serialize_enum_children() {
    #[derive(Facet, PartialEq, Debug)]
    struct Pipeline {
        #[facet(kdl::children)]
        steps: Vec<Step>,
    }

    #[derive(Facet, PartialEq, Debug)]
    struct Step {
        #[facet(kdl::argument)]
        name: String,
        #[facet(kdl::child)]
        action: Action,
    }

    #[derive(Facet, PartialEq, Debug)]
    #[repr(u8)]
    enum Action {
        Print {
            #[facet(kdl::property)]
            message: String,
        },
        Write {
            #[facet(kdl::property)]
            path: String,
        },
    }

    let pipeline = Pipeline {
        steps: vec![
            Step {
                name: "greet".to_string(),
                action: Action::Print {
                    message: "hello".to_string(),
                },
            },
            Step {
                name: "save".to_string(),
                action: Action::Write {
                    path: "/tmp/out.txt".to_string(),
                },
            },
        ],
    };

    let kdl = facet_kdl_legacy::to_string(&pipeline).unwrap();
    assert!(kdl.contains("step"));
    assert!(kdl.contains("\"greet\""));
    assert!(kdl.contains("Print"));
    assert!(kdl.contains("message=\"hello\""));
    assert!(kdl.contains("Write"));
    assert!(kdl.contains("path=\"/tmp/out.txt\""));

    // Round-trip test
    let parsed: Pipeline = facet_kdl_legacy::from_str(&kdl).unwrap();
    assert_eq!(parsed, pipeline);
}

/// Test serialization with kebab-case renaming.
#[test]
fn serialize_kebab_case() {
    #[derive(Facet, PartialEq, Debug)]
    #[facet(rename_all = "kebab-case")]
    struct Config {
        #[facet(kdl::child)]
        database_url: DatabaseUrl,
        #[facet(kdl::child)]
        #[facet(default)]
        max_connections: Option<MaxConnections>,
    }

    #[derive(Facet, PartialEq, Debug)]
    struct DatabaseUrl {
        #[facet(kdl::argument)]
        value: String,
    }

    #[derive(Facet, PartialEq, Debug)]
    struct MaxConnections {
        #[facet(kdl::argument)]
        value: u32,
    }

    let config = Config {
        database_url: DatabaseUrl {
            value: "postgres://localhost/db".to_string(),
        },
        max_connections: Some(MaxConnections { value: 100 }),
    };

    let kdl = facet_kdl_legacy::to_string(&config).unwrap();
    assert!(kdl.contains("database-url"));
    assert!(kdl.contains("max-connections"));

    // Round-trip test
    let parsed: Config = facet_kdl_legacy::from_str(&kdl).unwrap();
    assert_eq!(parsed, config);
}

/// Test serialization with node_name field for children.
#[test]
fn serialize_node_name_children() {
    #[derive(Facet, PartialEq, Debug)]
    struct Document {
        #[facet(kdl::child)]
        settings: Settings,
    }

    #[derive(Facet, PartialEq, Debug)]
    struct Settings {
        #[facet(kdl::children)]
        entries: Vec<Setting>,
    }

    #[derive(Facet, PartialEq, Debug)]
    struct Setting {
        #[facet(kdl::node_name)]
        key: String,
        #[facet(kdl::argument)]
        value: String,
    }

    let doc = Document {
        settings: Settings {
            entries: vec![
                Setting {
                    key: "log-level".to_string(),
                    value: "debug".to_string(),
                },
                Setting {
                    key: "timeout".to_string(),
                    value: "30s".to_string(),
                },
            ],
        },
    };

    let kdl = facet_kdl_legacy::to_string(&doc).unwrap();
    assert!(kdl.contains("log-level"));
    assert!(kdl.contains("\"debug\""));
    assert!(kdl.contains("timeout"));
    assert!(kdl.contains("\"30s\""));

    // Round-trip test
    let parsed: Document = facet_kdl_legacy::from_str(&kdl).unwrap();
    assert_eq!(parsed, doc);
}

/// Test serialization of boolean values.
#[test]
fn serialize_booleans() {
    #[derive(Facet, PartialEq, Debug)]
    struct Config {
        #[facet(kdl::child)]
        feature: Feature,
    }

    #[derive(Facet, PartialEq, Debug)]
    struct Feature {
        #[facet(kdl::property)]
        enabled: bool,
        #[facet(kdl::property)]
        optional: bool,
    }

    let config = Config {
        feature: Feature {
            enabled: true,
            optional: false,
        },
    };

    let kdl = facet_kdl_legacy::to_string(&config).unwrap();
    assert!(kdl.contains("enabled=#true"));
    assert!(kdl.contains("optional=#false"));

    // Round-trip test
    let parsed: Config = facet_kdl_legacy::from_str(&kdl).unwrap();
    assert_eq!(parsed, config);
}

/// Test serialization with nested child nodes.
#[test]
fn serialize_nested_children() {
    #[derive(Facet, PartialEq, Debug)]
    struct Root {
        #[facet(kdl::child)]
        level1: Level1,
    }

    #[derive(Facet, PartialEq, Debug)]
    struct Level1 {
        #[facet(kdl::argument)]
        name: String,
        #[facet(kdl::child)]
        level2: Level2,
    }

    #[derive(Facet, PartialEq, Debug)]
    struct Level2 {
        #[facet(kdl::argument)]
        value: i32,
    }

    let root = Root {
        level1: Level1 {
            name: "outer".to_string(),
            level2: Level2 { value: 42 },
        },
    };

    let kdl = facet_kdl_legacy::to_string(&root).unwrap();
    assert!(kdl.contains("level1"));
    assert!(kdl.contains("\"outer\""));
    assert!(kdl.contains("level2"));
    assert!(kdl.contains("42"));

    // Round-trip test
    let parsed: Root = facet_kdl_legacy::from_str(&kdl).unwrap();
    assert_eq!(parsed, root);
}

/// Test serialization escapes special characters in strings.
#[test]
fn serialize_string_escaping() {
    #[derive(Facet, PartialEq, Debug)]
    struct Config {
        #[facet(kdl::child)]
        message: Message,
    }

    #[derive(Facet, PartialEq, Debug)]
    struct Message {
        #[facet(kdl::argument)]
        text: String,
    }

    let config = Config {
        message: Message {
            text: "Hello\nWorld\t\"quoted\"".to_string(),
        },
    };

    let kdl = facet_kdl_legacy::to_string(&config).unwrap();
    // Should contain escaped sequences
    assert!(kdl.contains("\\n"));
    assert!(kdl.contains("\\t"));
    assert!(kdl.contains("\\\""));

    // Round-trip test
    let parsed: Config = facet_kdl_legacy::from_str(&kdl).unwrap();
    assert_eq!(parsed, config);
}
