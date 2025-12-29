use facet::Facet;
use std::convert::TryFrom;

/// Proxy for converting between Vec<String> and comma-separated string
/// This enables array properties in KDL using a simple string representation
#[derive(Facet, Clone)]
#[facet(transparent)]
pub struct FeaturesProxy(String);

impl TryFrom<FeaturesProxy> for Vec<String> {
    type Error = &'static str;

    fn try_from(proxy: FeaturesProxy) -> Result<Self, Self::Error> {
        if proxy.0.is_empty() {
            Ok(Vec::new())
        } else {
            Ok(proxy.0.split(',').map(String::from).collect())
        }
    }
}

impl From<Vec<String>> for FeaturesProxy {
    fn from(features: Vec<String>) -> Self {
        Self(features.join(","))
    }
}

impl From<&Vec<String>> for FeaturesProxy {
    fn from(features: &Vec<String>) -> Self {
        Self(features.join(","))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet_kdl_legacy as kdl;

    #[derive(Facet, PartialEq, Debug)]
    struct DependencySpec {
        #[facet(kdl::node_name)]
        pub name: String,

        #[facet(kdl::argument)]
        pub version: String,

        #[facet(kdl::property, default, proxy = FeaturesProxy)]
        pub features: Vec<String>,

        #[facet(kdl::property, default)]
        pub git: Option<String>,
    }

    #[derive(Facet, PartialEq, Debug)]
    struct DodecaConfig {
        #[facet(kdl::child)]
        dependencies: Dependencies,
    }

    #[derive(Facet, PartialEq, Debug)]
    struct Dependencies {
        #[facet(kdl::children)]
        pub deps: Vec<DependencySpec>,
    }

    #[test]
    fn test_proxy_array_properties() {
        let kdl_input = r#"
            dependencies {
                serde "1.0" features="derive,alloc"
                tokio "1.0" git="https://github.com/tokio-rs/tokio" features="full,macros,io-util"
                simple "0.1"
            }
        "#;

        let result: Result<DodecaConfig, _> = facet_kdl_legacy::from_str(kdl_input);
        match result {
            Ok(config) => {
                println!(
                    "✅ Successfully parsed dodeca config with proxy: {:?}",
                    config
                );
                assert_eq!(config.dependencies.deps.len(), 3);

                // Test serde with features
                let serde_dep = &config.dependencies.deps[0];
                assert_eq!(serde_dep.name, "serde");
                assert_eq!(serde_dep.version, "1.0");
                assert_eq!(
                    serde_dep.features,
                    vec!["derive".to_string(), "alloc".to_string()]
                );

                // Test tokio with git and features
                let tokio_dep = &config.dependencies.deps[1];
                assert_eq!(tokio_dep.name, "tokio");
                assert_eq!(tokio_dep.version, "1.0");
                assert_eq!(
                    tokio_dep.git,
                    Some("https://github.com/tokio-rs/tokio".to_string())
                );
                assert_eq!(
                    tokio_dep.features,
                    vec![
                        "full".to_string(),
                        "macros".to_string(),
                        "io-util".to_string()
                    ]
                );

                // Test simple dependency without features (should be empty vec due to default)
                let simple_dep = &config.dependencies.deps[2];
                assert_eq!(simple_dep.name, "simple");
                assert_eq!(simple_dep.version, "0.1");
                assert_eq!(simple_dep.features, Vec::<String>::new());

                println!("✅ Proxy-based array properties work perfectly!");
            }
            Err(e) => {
                println!("❌ Failed to deserialize: {:?}", e);
                panic!("Proxy solution should work");
            }
        }
    }

    #[test]
    fn test_proxy_edge_cases() {
        let kdl_input = r#"
            dependencies {
                empty_features "1.0" features=""
                single_feature "2.0" features="only-one"
            }
        "#;

        let result: Result<DodecaConfig, _> = facet_kdl_legacy::from_str(kdl_input);
        match result {
            Ok(config) => {
                // Empty features should become empty vector
                let empty = &config.dependencies.deps[0];
                assert_eq!(empty.features, Vec::<String>::new());

                // Single feature should work
                let single = &config.dependencies.deps[1];
                assert_eq!(single.features, vec!["only-one".to_string()]);
            }
            Err(e) => {
                println!("❌ Edge case test failed: {:?}", e);
                panic!("Edge cases should work");
            }
        }
    }
}
