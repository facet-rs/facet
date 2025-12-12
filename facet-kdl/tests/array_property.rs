use facet::Facet;
use facet_kdl as kdl;

#[test]
fn test_array_property_parsing() {
    #[derive(Facet, Debug, PartialEq)]
    struct DependencySpec {
        #[facet(kdl::argument)]
        name: String,
        #[facet(kdl::argument)]
        version: String,
        #[facet(kdl::property, default)]
        features: Option<Vec<String>>,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::children)]
        dependencies: Vec<DependencySpec>,
    }

    // Test the syntax that should work according to the issue
    let kdl = r#"
        dependencies {
            serde "1.0"
            tokio "1.0" features=["full"]
        }
    "#;

    let result: Result<Config, _> = facet_kdl::from_str(kdl);

    // This should work but currently fails
    assert!(
        result.is_ok(),
        "Array property parsing failed: {:?}",
        result
    );

    let config = result.unwrap();
    assert_eq!(config.dependencies.len(), 2);
    assert_eq!(config.dependencies[0].name, "serde");
    assert_eq!(config.dependencies[1].name, "tokio");
    assert_eq!(
        config.dependencies[1].features,
        Some(vec!["full".to_string()])
    );
}
