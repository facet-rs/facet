use facet::Facet;
use facet_kdl_legacy as kdl;
use indoc::indoc;

// ============================================================================
// deny_unknown_fields support
// ============================================================================

/// Test that unknown properties are skipped by default (without #[facet(deny_unknown_fields)])
#[test]
fn unknown_properties_skipped_by_default() {
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
        port: u16,
    }

    // KDL has an unknown property 'timeout' which should be silently skipped
    let kdl = indoc! {r#"
        server "localhost" port=8080 timeout=30 unknown_prop="ignored"
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).unwrap();
    assert_eq!(config.server.host, "localhost");
    assert_eq!(config.server.port, 8080);
}

/// Test that #[facet(deny_unknown_fields)] causes an error on unknown properties
#[test]
fn deny_unknown_fields_rejects_unknown_properties() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        server: Server,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[facet(deny_unknown_fields)]
    struct Server {
        #[facet(kdl::argument)]
        host: String,
        #[facet(kdl::property)]
        port: u16,
    }

    // KDL has an unknown property 'timeout'
    let kdl = indoc! {r#"
        server "localhost" port=8080 timeout=30
    "#};

    let result: Result<Config, _> = facet_kdl_legacy::from_str(kdl);
    assert!(result.is_err(), "should error on unknown property");
    let err = result.unwrap_err();
    let err_msg = err.to_string();
    eprintln!("Error message: {err_msg}");
    // Error should mention the unknown property and expected fields
    assert!(
        err_msg.contains("timeout") && err_msg.contains("unknown"),
        "error should mention unknown property 'timeout': {err_msg}"
    );
}

/// Test that known properties still work with deny_unknown_fields
#[test]
fn deny_unknown_fields_allows_known_properties() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        server: Server,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[facet(deny_unknown_fields)]
    struct Server {
        #[facet(kdl::argument)]
        host: String,
        #[facet(kdl::property)]
        port: u16,
        #[facet(kdl::property, default)]
        timeout: Option<u32>,
    }

    let kdl = indoc! {r#"
        server "localhost" port=8080 timeout=30
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).unwrap();
    assert_eq!(config.server.host, "localhost");
    assert_eq!(config.server.port, 8080);
    assert_eq!(config.server.timeout, Some(30));
}

/// Test deny_unknown_fields with flattened structs (solver path)
#[test]
fn deny_unknown_fields_with_flatten() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        server: Server,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[facet(deny_unknown_fields)]
    struct Server {
        #[facet(kdl::argument)]
        name: String,
        #[facet(flatten)]
        connection: ConnectionSettings,
    }

    #[derive(Facet, Debug, PartialEq, Default)]
    struct ConnectionSettings {
        #[facet(kdl::property, default)]
        host: String,
        #[facet(kdl::property, default)]
        port: u16,
    }

    // Unknown property should error with deny_unknown_fields + flatten
    let kdl = indoc! {r#"
        server "main" host="localhost" port=8080 unknown_field="bad"
    "#};

    let result: Result<Config, _> = facet_kdl_legacy::from_str(kdl);
    assert!(
        result.is_err(),
        "should error on unknown property with flatten"
    );
}

/// Test that unknown child nodes are skipped by default
#[test]
fn unknown_child_nodes_skipped_by_default() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        server: Server,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Server {
        #[facet(kdl::argument)]
        host: String,
    }

    // KDL has an unknown child node 'unknown_section' which should be silently skipped
    let kdl = indoc! {r#"
        server "localhost"
        unknown_section {
            data "ignored"
        }
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).unwrap();
    assert_eq!(config.server.host, "localhost");
}

/// Test that deny_unknown_fields rejects unknown child nodes
#[test]
fn deny_unknown_fields_rejects_unknown_child_nodes() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(deny_unknown_fields)]
    struct Config {
        #[facet(kdl::child)]
        server: Server,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Server {
        #[facet(kdl::argument)]
        host: String,
    }

    // KDL has an unknown child node 'unknown_section'
    let kdl = indoc! {r#"
        server "localhost"
        unknown_section {
            data "ignored"
        }
    "#};

    let result: Result<Config, _> = facet_kdl_legacy::from_str(kdl);
    assert!(result.is_err(), "should error on unknown child node");
    let err = result.unwrap_err();
    let err_msg = err.to_string();
    eprintln!("Error message: {err_msg}");
    assert!(
        err_msg.contains("unknown_section"),
        "error should mention unknown child node: {err_msg}"
    );
}
