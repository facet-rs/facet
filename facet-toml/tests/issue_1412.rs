// Test for issue #1412: facet-toml fails to parse empty TOML tables
use facet::Facet;
use facet_value::Value;
use std::collections::HashMap;

#[derive(Facet, Debug)]
struct PackageProfile {
    opt_level: Option<i64>,
}

#[derive(Facet, Debug)]
struct Profile {
    package: Option<HashMap<String, PackageProfile>>,
}

#[derive(Facet, Debug)]
struct Config {
    profile: HashMap<String, Profile>,
}

#[test]
fn test_empty_toml_table() {
    let toml = r#"
[profile.release.package]
# zed = { codegen-units = 16 }
"#;

    // This should parse as an empty HashMap
    let result = facet_toml::from_str::<Config>(toml);
    match &result {
        Ok(config) => {
            assert!(config.profile.contains_key("release"));
            let release_profile = &config.profile["release"];
            assert!(release_profile.package.is_some());
            assert!(release_profile.package.as_ref().unwrap().is_empty());
        }
        Err(e) => {
            panic!("Failed to parse: {:?}", e);
        }
    }
}

#[test]
fn test_empty_toml_table_at_root() {
    #[derive(Facet, Debug)]
    struct RootConfig {
        empty_section: Option<HashMap<String, String>>,
    }

    let toml = r#"
[empty_section]
# All fields commented out
"#;

    let config: RootConfig = facet_toml::from_str(toml).unwrap();
    assert!(config.empty_section.is_some());
    assert!(config.empty_section.as_ref().unwrap().is_empty());
}

#[test]
fn test_empty_toml_table_with_value_type() {
    // This is the actual bug - when a table is empty and we're deserializing to Value
    #[derive(Facet, Debug)]
    struct Config {
        metadata: Option<Value>,
        other: Option<String>,
    }

    // Try with navigation AFTER the empty table
    let toml = r#"
[metadata]
# All fields commented out

[other_section]
value = "test"
"#;

    let result = facet_toml::from_str::<Config>(toml);
    match &result {
        Ok(config) => {
            assert!(config.metadata.is_some());
            // The empty table should deserialize as an empty object/map
            let val = config.metadata.as_ref().unwrap();
            assert!(val.is_object(), "Expected object, got: {:?}", val);
            let obj = val.as_object().unwrap();
            assert!(obj.is_empty(), "Expected empty object, got: {:?}", obj);
        }
        Err(e) => {
            panic!("Failed to parse empty table into Value: {:?}", e);
        }
    }
}

#[test]
fn test_exact_issue_case() {
    // Reproduce the EXACT case from the issue
    #[derive(Facet, Debug)]
    struct PackageProfile {
        opt_level: Option<i64>,
    }

    #[derive(Facet, Debug)]
    struct Profile {
        package: Option<HashMap<String, PackageProfile>>,
    }

    #[derive(Facet, Debug)]
    struct Config {
        profile: HashMap<String, Profile>,
    }

    let toml = r#"
[profile.release.package]
# zed = { codegen-units = 16 }

[profile.release-fast]
inherits = "release"
"#;

    let result = facet_toml::from_str::<Config>(toml);
    match &result {
        Ok(config) => {
            println!("Success! Parsed: {:?}", config);
            assert!(config.profile.contains_key("release"));
            assert!(config.profile.contains_key("release-fast"));
        }
        Err(e) => {
            panic!("Failed to parse: {:?}", e);
        }
    }
}
