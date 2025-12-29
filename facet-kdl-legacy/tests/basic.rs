#![allow(missing_docs)]

use facet::Facet;
use facet_kdl_legacy as kdl;
use indoc::indoc;

#[test]
fn it_works() {
    // one test must pass
}

#[test]
fn basic_node() {
    // QUESTION: I don't know when this would be particularly good practice, but it could be nice if `facet` shipped
    // some sort of macro that allowed libraries to rename the Facet trait / attributes.unwrap() This might make it clearer
    // what's going on if you're ever mixing several `Facet` libraries that all use different arbitrary attributes.unwrap() I
    // just think that `#[kdl(child)]` would be a lot clearer than `#[facet(kdl::child)]` if, say, you also wanted to
    // deserialize from something like XML.unwrap() Or command-line arguments.unwrap() Those would also need attributes, e.g.
    // `#[facet(text)]` or `#[facet(positional)]`, and I think things would be a lot clearer as `#[xml(text)]` and
    // `#[args(positional)]`. If, however, it's far too evil or hard to implment something like that, then arbitrary
    // attributes should be given "namespaces", maybe.unwrap() Like `#[facet(kdl, child)]` or `#[facet(xml, text)].unwrap()
    //
    // Overall I think this is a hard design question, but I do think it's worth considering how several `facet` crates
    // relying on arbitrary attributes should interact...
    #[derive(Facet)]
    struct Basic {
        #[facet(kdl::child)]
        title: Title,
    }

    #[derive(Facet)]
    struct Title {
        #[facet(kdl::argument)]
        title: String,
    }

    let kdl = indoc! {r#"
        title "Hello, World"
    "#};

    dbg!(Basic::SHAPE);

    let _basic: Basic = facet_kdl_legacy::from_str(kdl).unwrap();
}

#[test]
fn canon_example() {
    #[derive(Facet, PartialEq, Debug)]
    struct Root {
        #[facet(kdl::child)]
        package: Package,
    }

    #[derive(Facet, PartialEq, Debug)]
    struct Package {
        #[facet(kdl::child)]
        name: Name,
        #[facet(kdl::child)]
        version: Version,
        #[facet(kdl::child)]
        dependencies: Dependencies,
        #[facet(kdl::child)]
        scripts: Scripts,
        #[facet(kdl::child)]
        #[facet(rename = "the-matrix")]
        the_matrix: TheMatrix,
    }

    #[derive(Facet, PartialEq, Debug)]
    struct Name {
        #[facet(kdl::argument)]
        name: String,
    }

    #[derive(Facet, PartialEq, Debug)]
    struct Version {
        #[facet(kdl::argument)]
        version: String,
    }

    #[derive(Facet, PartialEq, Debug)]
    struct Dependencies {
        #[facet(kdl::children)]
        dependencies: Vec<Dependency>,
    }

    #[derive(Facet, PartialEq, Debug)]
    struct Scripts {
        #[facet(kdl::children)]
        scripts: Vec<Script>,
    }

    #[derive(Facet, PartialEq, Debug)]
    struct TheMatrix {
        #[facet(kdl::arguments)]
        data: Vec<u8>,
    }

    #[derive(Facet, PartialEq, Debug)]
    struct Dependency {
        #[facet(kdl::node_name)]
        name: String,
        #[facet(kdl::argument)]
        version: String,
        #[facet(kdl::property)]
        optional: Option<bool>,
        #[facet(kdl::property)]
        alias: Option<String>,
    }

    #[derive(Facet, PartialEq, Debug)]
    struct Script {
        #[facet(kdl::node_name)]
        name: String,
        #[facet(kdl::argument)]
        body: String,
    }

    let kdl = indoc! {r##"
        package {
            name my-pkg
            version "1.2.3"

            dependencies {
                // Nodes can have standalone values as well as
                // key/value pairs.
                lodash "^3.2.1" optional=#true alias=underscore
            }

            scripts {
                // "Raw" and dedented multi-line strings are supported.
                message """
                    hello
                    world
                    """
                build #"""
                    echo "foo"
                    node -c "console.log('hello, world!');"
                    echo "foo" > some-file.txt
                    """#
            }

            // `\` breaks up a single node across multiple lines.
            the-matrix 1 2 3 \
                       4 5 6 \
                       7 8 9

            // "Slashdash" comments operate at the node level,
            // with just `/-`.
            /-this-is-commented {
                this entire node {
                    is gone
                }
            }
        }
    "##};

    let root: Root = facet_kdl_legacy::from_str(kdl).unwrap();
    assert_eq!(
        root,
        Root {
            package: Package {
                name: Name {
                    name: "my-pkg".to_string()
                },
                version: Version {
                    version: "1.2.3".to_string()
                },
                dependencies: Dependencies {
                    dependencies: vec![Dependency {
                        name: "lodash".to_string(),
                        version: "^3.2.1".to_string(),
                        optional: Some(true),
                        alias: Some("underscore".to_string())
                    }]
                },
                scripts: Scripts {
                    scripts: vec![
                        Script {
                            name: "message".to_string(),
                            body: "hello\nworld".to_string()
                        },
                        Script {
                            name: "build".to_string(),
                            body: indoc! {r#"
                                echo "foo"
                                node -c "console.log('hello, world!');"
                                echo "foo" > some-file.txt"#}
                            .to_string()
                        }
                    ]
                },
                the_matrix: TheMatrix {
                    data: vec![1, 2, 3, 4, 5, 6, 7, 8, 9]
                },
            }
        }
    );
}

/// Test key-value map pattern using node_name + children.
/// Useful for settings, environment variables, or any dynamic key-value structure.
#[test]
fn key_value_map_with_node_name() {
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

    let kdl = indoc! {r#"
        settings {
            log-level "debug"
            timeout "30s"
            feature.new-ui "enabled"
        }
    "#};

    let doc: Document = facet_kdl_legacy::from_str(kdl).unwrap();

    assert_eq!(doc.settings.entries.len(), 3);
    assert_eq!(doc.settings.entries[0].key, "log-level");
    assert_eq!(doc.settings.entries[0].value, "debug");
    assert_eq!(doc.settings.entries[1].key, "timeout");
    assert_eq!(doc.settings.entries[1].value, "30s");
    assert_eq!(doc.settings.entries[2].key, "feature.new-ui");
    assert_eq!(doc.settings.entries[2].value, "enabled");
}

/// Test raw strings for embedded expressions/formulas.
/// Raw strings preserve quotes and special characters without escaping.
#[test]
fn raw_string_expression() {
    #[derive(Facet, PartialEq, Debug)]
    #[facet(rename_all = "kebab-case")]
    struct Rule {
        #[facet(kdl::argument)]
        name: String,
        #[facet(kdl::child)]
        #[facet(default)]
        condition: Option<Condition>,
    }

    #[derive(Facet, PartialEq, Debug)]
    struct Condition {
        #[facet(kdl::argument)]
        expr: String,
    }

    #[derive(Facet, PartialEq, Debug)]
    struct RuleSet {
        #[facet(kdl::children)]
        rules: Vec<Rule>,
    }

    // Raw strings let you embed expressions with quotes without escaping
    let kdl = indoc! {r##"
        rule "check-platform" {
            condition #"(eq platform "linux")"#
        }
        rule "complex-check" {
            condition #"(and (gte version "2.0") (contains features "beta"))"#
        }
    "##};

    let rules: RuleSet = facet_kdl_legacy::from_str(kdl).unwrap();

    assert_eq!(rules.rules.len(), 2);
    assert_eq!(rules.rules[0].name, "check-platform");
    assert_eq!(
        rules.rules[0].condition.as_ref().unwrap().expr,
        r#"(eq platform "linux")"#
    );
    assert_eq!(rules.rules[1].name, "complex-check");
    assert_eq!(
        rules.rules[1].condition.as_ref().unwrap().expr,
        r#"(and (gte version "2.0") (contains features "beta"))"#
    );
}

/// Test that #[facet(skip)] fields are ignored during deserialization
/// and get their default value.
#[test]
fn skip_field() {
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
        #[facet(skip)]
        internal_id: u64, // Should be skipped and get default value
    }

    let kdl = indoc! {r#"
        server "localhost" port=8080
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).unwrap();
    assert_eq!(config.server.host, "localhost");
    assert_eq!(config.server.port, 8080);
    assert_eq!(config.server.internal_id, 0); // Default value
}

/// Test child nodes with arguments into nested structs - the common KDL pattern.
/// This tests the bug where `repo "value"` style nodes couldn't be deserialized
/// into nested struct types with `#[facet(kdl::argument)]` fields.
#[test]
fn test_child_node_with_argument() {
    #[derive(Debug, Facet, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        repo: Repo,
        #[facet(kdl::child)]
        commit: Commit,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Repo {
        #[facet(kdl::argument)]
        value: String,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Commit {
        #[facet(kdl::argument)]
        value: String,
    }

    let kdl = indoc! {r#"
        repo "https://github.com/example/repo"
        commit "abc123"
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).unwrap();
    assert_eq!(config.repo.value, "https://github.com/example/repo");
    assert_eq!(config.commit.value, "abc123");
}

/// Test that optional child fields with `#[facet(default)]` work when the child is omitted.
/// Regression test for: Option<T> with #[facet(kdl::child)] + #[facet(default)] fails when child is omitted
#[test]
fn test_optional_child_with_default() {
    #[derive(Debug, Facet, PartialEq)]
    struct Authors {
        #[facet(kdl::argument)]
        value: String,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Repo {
        #[facet(kdl::argument)]
        value: String,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        repo: Repo,
        #[facet(kdl::child)]
        #[facet(default)]
        authors: Option<Authors>,
    }

    // Test with authors omitted - should deserialize to None
    let kdl_without_authors = r#"repo "https://example.com""#;
    let config: Config = facet_kdl_legacy::from_str(kdl_without_authors).unwrap();
    assert_eq!(config.repo.value, "https://example.com");
    assert_eq!(config.authors, None);

    // Test with authors present - should deserialize to Some
    let kdl_with_authors = indoc! {r#"
        repo "https://example.com"
        authors "Alice"
    "#};
    let config: Config = facet_kdl_legacy::from_str(kdl_with_authors).unwrap();
    assert_eq!(config.repo.value, "https://example.com");
    assert_eq!(
        config.authors,
        Some(Authors {
            value: "Alice".to_string()
        })
    );
}

/// Test the exact pattern from the bug report - top-level child nodes with arguments.
/// The KDL pattern: `repo "value"` and `commit "value"` as direct children of the document.
#[test]
fn test_top_level_child_nodes_with_arguments() {
    #[derive(Debug, Facet, PartialEq)]
    struct Repo {
        #[facet(kdl::argument)]
        value: String,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Commit {
        #[facet(kdl::argument)]
        value: String,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        repo: Repo,
        #[facet(kdl::child)]
        commit: Commit,
        #[facet(kdl::child)]
        license: License,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct License {
        #[facet(kdl::argument)]
        value: String,
    }

    let kdl = indoc! {r#"
        repo "https://github.com/example/repo"
        commit "abc123def456"
        license "MIT"
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).unwrap();
    assert_eq!(config.repo.value, "https://github.com/example/repo");
    assert_eq!(config.commit.value, "abc123def456");
    assert_eq!(config.license.value, "MIT");
}

/// Test child nodes with arguments when flattened structs are involved.
/// This forces the solver-based deserialization path.
#[test]
fn test_child_with_argument_and_flatten_struct() {
    #[derive(Debug, Facet, PartialEq)]
    struct Repo {
        #[facet(kdl::argument)]
        url: String,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct ConnectionSettings {
        #[facet(kdl::property)]
        timeout: u32,
        #[facet(kdl::property)]
        retries: u8,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        server: Server,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Server {
        #[facet(kdl::child)]
        repo: Repo,
        #[facet(flatten)]
        connection: ConnectionSettings,
    }

    // Test: server has a child `repo` with an argument, and flattened connection properties
    let kdl = indoc! {r#"
        server timeout=30 retries=3 {
            repo "https://github.com/example/repo"
        }
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).unwrap();
    assert_eq!(config.server.repo.url, "https://github.com/example/repo");
    assert_eq!(config.server.connection.timeout, 30);
    assert_eq!(config.server.connection.retries, 3);
}

/// Test the exact pattern from the bug report - using `#[facet(kdl::child, rename = "...")]`
/// on the struct itself, not just `#[facet(kdl::child)]` on the field.
#[test]
fn test_child_node_with_argument_using_struct_rename() {
    #[derive(Debug, Facet, PartialEq)]
    #[facet(kdl::child, rename = "repo")]
    struct Repo {
        #[facet(kdl::argument)]
        pub value: String,
    }

    #[derive(Debug, Facet, PartialEq)]
    #[facet(kdl::child, rename = "commit")]
    struct Commit {
        #[facet(kdl::argument)]
        pub value: String,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        pub repo: Repo,
        #[facet(kdl::child)]
        pub commit: Commit,
    }

    let kdl = indoc! {r#"
        repo "https://github.com/example/repo"
        commit "abc123"
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).unwrap();
    assert_eq!(config.repo.value, "https://github.com/example/repo");
    assert_eq!(config.commit.value, "abc123");
}
