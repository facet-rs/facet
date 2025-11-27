#![allow(missing_docs)]

use facet::Facet;
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
    // just think that `#[kdl(child)]` would be a lot clearer than `#[facet(child)]` if, say, you also wanted to
    // deserialize from something like XML.unwrap() Or command-line arguments.unwrap() Those would also need attributes, e.g.
    // `#[facet(text)]` or `#[facet(positional)]`, and I think things would be a lot clearer as `#[xml(text)]` and
    // `#[args(positional)]`. If, however, it's far too evil or hard to implment something like that, then arbitrary
    // attributes should be given "namespaces", maybe.unwrap() Like `#[facet(kdl, child)]` or `#[facet(xml, text)].unwrap()
    //
    // Overall I think this is a hard design question, but I do think it's worth considering how several `facet` crates
    // relying on arbitrary attributes should interact...
    #[derive(Facet)]
    struct Basic {
        #[facet(child)]
        title: Title,
    }

    #[derive(Facet)]
    struct Title {
        #[facet(argument)]
        title: String,
    }

    let kdl = indoc! {r#"
        title "Hello, World"
    "#};

    dbg!(Basic::SHAPE);

    let _basic: Basic = facet_kdl::from_str(kdl).unwrap();
}

#[test]
fn canon_example() {
    #[derive(Facet, PartialEq, Debug)]
    struct Root {
        #[facet(child)]
        package: Package,
    }

    #[derive(Facet, PartialEq, Debug)]
    struct Package {
        #[facet(child)]
        name: Name,
        #[facet(child)]
        version: Version,
        #[facet(child)]
        dependencies: Dependencies,
        #[facet(child)]
        scripts: Scripts,
        #[facet(child)]
        #[facet(rename = "the-matrix")]
        the_matrix: TheMatrix,
    }

    #[derive(Facet, PartialEq, Debug)]
    struct Name {
        #[facet(argument)]
        name: String,
    }

    #[derive(Facet, PartialEq, Debug)]
    struct Version {
        #[facet(argument)]
        version: String,
    }

    #[derive(Facet, PartialEq, Debug)]
    struct Dependencies {
        #[facet(children)]
        dependencies: Vec<Dependency>,
    }

    #[derive(Facet, PartialEq, Debug)]
    struct Scripts {
        #[facet(children)]
        scripts: Vec<Script>,
    }

    #[derive(Facet, PartialEq, Debug)]
    struct TheMatrix {
        #[facet(arguments)]
        data: Vec<u8>,
    }

    #[derive(Facet, PartialEq, Debug)]
    struct Dependency {
        #[facet(node_name)]
        name: String,
        #[facet(argument)]
        version: String,
        #[facet(property)]
        optional: Option<bool>,
        #[facet(property)]
        alias: Option<String>,
    }

    #[derive(Facet, PartialEq, Debug)]
    struct Script {
        #[facet(node_name)]
        name: String,
        #[facet(argument)]
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

    let root: Root = facet_kdl::from_str(kdl).unwrap();
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
        #[facet(child)]
        settings: Settings,
    }

    #[derive(Facet, PartialEq, Debug)]
    struct Settings {
        #[facet(children)]
        entries: Vec<Setting>,
    }

    #[derive(Facet, PartialEq, Debug)]
    struct Setting {
        #[facet(node_name)]
        key: String,
        #[facet(argument)]
        value: String,
    }

    let kdl = indoc! {r#"
        settings {
            log-level "debug"
            timeout "30s"
            feature.new-ui "enabled"
        }
    "#};

    let doc: Document = facet_kdl::from_str(kdl).unwrap();

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
        #[facet(argument)]
        name: String,
        #[facet(child)]
        #[facet(default)]
        condition: Option<Condition>,
    }

    #[derive(Facet, PartialEq, Debug)]
    struct Condition {
        #[facet(argument)]
        expr: String,
    }

    #[derive(Facet, PartialEq, Debug)]
    struct RuleSet {
        #[facet(children)]
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

    let rules: RuleSet = facet_kdl::from_str(kdl).unwrap();

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
        #[facet(child)]
        server: Server,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Server {
        #[facet(argument)]
        host: String,
        #[facet(property)]
        port: u16,
        #[facet(skip)]
        internal_id: u64, // Should be skipped and get default value
    }

    let kdl = indoc! {r#"
        server "localhost" port=8080
    "#};

    let config: Config = facet_kdl::from_str(kdl).unwrap();
    assert_eq!(config.server.host, "localhost");
    assert_eq!(config.server.port, 8080);
    assert_eq!(config.server.internal_id, 0); // Default value
}
