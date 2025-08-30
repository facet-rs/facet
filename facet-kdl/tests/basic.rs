use facet::Facet;
use facet_testhelpers::test;
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
