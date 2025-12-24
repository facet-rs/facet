/// Test for issue #1446: Array-of-tables fails when field is facet_value::Value
///
/// When a field is typed as `facet_value::Value` and the TOML contains multiple
/// `[[array.of.tables]]` entries, facet-format-toml was failing with:
/// "reflection error: Operation failed on shape Value: begin_list can only be called on List types"
///
/// The issue was that when TOML table reopening found an existing field (like metadata),
/// begin_object_entry would set tracker = Tracker::Scalar even for DynamicValue types.
/// Later, begin_list would fail because it didn't handle Scalar trackers on DynamicValue shapes.
use facet::Facet;

#[derive(Facet, Debug, Clone)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub metadata: Option<facet_value::Value>,
}

#[derive(Facet, Debug, Clone)]
pub struct Manifest {
    pub package: Package,
}

#[test]
fn test_array_of_tables_in_value() {
    let toml = r#"
[package]
name = "test"
version = "0.1.0"

[[package.metadata.release.pre-release-replacements]]
file = "CHANGELOG.md"
search = "Unreleased"

[[package.metadata.release.pre-release-replacements]]
file = "CHANGELOG.md"
search = "HEAD"
"#;

    let result = facet_format_toml::from_str::<Manifest>(toml);
    match &result {
        Ok(manifest) => {
            assert_eq!(manifest.package.name, "test");
            assert_eq!(manifest.package.version, "0.1.0");
            assert!(manifest.package.metadata.is_some());
            let metadata = manifest.package.metadata.as_ref().unwrap();

            // Verify the structure
            assert!(metadata.is_object());
            let obj = metadata.as_object().unwrap();
            assert!(obj.contains_key("release"));

            let release = obj.get("release").unwrap();
            assert!(release.is_object());
            let release_obj = release.as_object().unwrap();
            assert!(release_obj.contains_key("pre-release-replacements"));

            let replacements = release_obj.get("pre-release-replacements").unwrap();
            assert!(replacements.is_array());
            let array = replacements.as_array().unwrap();
            assert_eq!(array.len(), 2, "Should have 2 replacement entries");
        }
        Err(e) => {
            panic!("Failed to parse array-of-tables in Value field: {:?}", e);
        }
    }
}

#[test]
fn test_simple_array_of_tables_in_value() {
    // Simplified test case
    #[derive(Facet, Debug)]
    struct Config {
        data: facet_value::Value,
    }

    let toml = r#"
[[data.items]]
name = "first"

[[data.items]]
name = "second"
"#;

    let result = facet_format_toml::from_str::<Config>(toml);
    match &result {
        Ok(config) => {
            assert!(config.data.is_object());
            let obj = config.data.as_object().unwrap();
            assert!(obj.contains_key("items"));

            let items = obj.get("items").unwrap();
            assert!(items.is_array());
            let array = items.as_array().unwrap();
            assert_eq!(array.len(), 2, "Should have 2 items");
        }
        Err(e) => {
            panic!(
                "Failed to parse simple array-of-tables in Value field: {:?}",
                e
            );
        }
    }
}
