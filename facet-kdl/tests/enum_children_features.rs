use facet::Facet;
use facet_kdl as kdl;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Facet)]
#[repr(u8)]
pub enum DependencyChild {
    Features {
        #[facet(kdl::arguments)]
        features: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Facet)]
pub struct DependencySpec {
    /// Crate name (from KDL node name)
    #[facet(kdl::node_name)]
    pub name: String,

    /// Version requirement (positional argument)
    #[facet(kdl::argument)]
    pub version: String,

    /// Git repository URL (optional)
    #[facet(kdl::property, default)]
    pub git: Option<String>,

    /// Git revision/commit hash (optional)
    #[facet(kdl::property, default)]
    pub rev: Option<String>,

    /// Git branch (optional)
    #[facet(kdl::property, default)]
    pub branch: Option<String>,

    /// Local path (optional, relative to project root)
    #[facet(kdl::property, default)]
    pub path: Option<String>,

    /// Crate features to enable (optional)
    #[facet(kdl::children)]
    pub children: Vec<DependencyChild>,
}

impl DependencySpec {
    pub fn get_features(&self) -> Vec<&str> {
        self.children
            .iter()
            .flat_map(|child| match child {
                DependencyChild::Features { features } => features.as_slice(),
            })
            .map(|s| s.as_str())
            .collect()
    }
}

#[test]
fn test_enum_children_array_parsing() {
    // Test the direct pattern that works
    #[derive(Facet, PartialEq, Debug)]
    struct TestContainer {
        #[facet(kdl::children)]
        children: Vec<DependencyChild>,
    }

    let kdl_input = r#"
        test {
            Features "derive" "alloc"
        }
        "#;

    let result: Result<TestContainer, _> = kdl::from_str(kdl_input);
    match result {
        Ok(container) => {
            println!("✅ Successfully parsed: {:?}", container);
            assert_eq!(container.children.len(), 1);
            let DependencyChild::Features { features } = &container.children[0];
            assert_eq!(features, &vec!["derive".to_string(), "alloc".to_string()]);
            println!("✅ Successfully parsed features: {:?}", features);
        }
        Err(e) => {
            println!("❌ Failed to deserialize: {:?}", e);
            panic!("Failed to deserialize dependency with enum children");
        }
    }
}

#[test]
fn test_dependency_spec_with_features() {
    // Test the actual DependencySpec with enum children
    let kdl_input = r#"
        serde "1.0" {
            Features "derive" "alloc"
        }
        "#;

    let result: Result<DependencySpec, _> = kdl::from_str(kdl_input);
    match result {
        Ok(spec) => {
            println!("✅ Successfully parsed DependencySpec: {:?}", spec);
            assert_eq!(spec.name, "serde");
            assert_eq!(spec.version, "1.0");
            let features = spec.get_features();
            assert_eq!(features, vec!["derive", "alloc"]);
            println!(
                "✅ Successfully parsed dependency with features: {:?}",
                features
            );
        }
        Err(e) => {
            println!("❌ Failed to deserialize: {:?}", e);
            panic!("Failed to deserialize DependencySpec with enum children");
        }
    }
}

#[test]
fn test_dependency_spec_without_features() {
    // Test DependencySpec without any children
    let kdl_input = r#"
        serde "1.0"
        "#;

    let result: Result<DependencySpec, _> = kdl::from_str(kdl_input);
    match result {
        Ok(spec) => {
            println!("✅ Successfully parsed DependencySpec: {:?}", spec);
            assert_eq!(spec.name, "serde");
            assert_eq!(spec.version, "1.0");
            let features = spec.get_features();
            assert_eq!(features, Vec::<&str>::new());
            println!(
                "✅ Successfully parsed dependency without features: {:?}",
                features
            );
        }
        Err(e) => {
            println!("❌ Failed to deserialize: {:?}", e);
            panic!("Failed to deserialize DependencySpec without children");
        }
    }
}
