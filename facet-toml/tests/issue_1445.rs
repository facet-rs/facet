// Test for issue #1445: facet-toml #[facet(flatten)] Value field fails when table has only known fields
use facet::Facet;
use facet_value::Value;

#[derive(Facet, Debug, Clone)]
pub struct Badge {
    #[facet(flatten)]
    pub attributes: Value,
}

#[derive(Facet, Debug)]
pub struct Config {
    pub badges: std::collections::HashMap<String, Badge>,
}

#[test]
fn test_flatten_value_with_only_known_fields() {
    // This is the minimal case from the issue:
    // A table with only known (non-flattened) fields
    let toml = r#"
[badges.appveyor]
repository = "user/repo"
"#;

    // This should not fail with "Field 'Badge::attributes' was not initialized"
    let result = facet_toml::from_str::<Config>(toml);
    match &result {
        Ok(config) => {
            println!("Success! Parsed: {:?}", config);
            assert!(config.badges.contains_key("appveyor"));
            let badge = &config.badges["appveyor"];

            // The attributes field should be initialized even if empty
            // (or containing the known fields, depending on implementation)
            println!("Badge attributes: {:?}", badge.attributes);
        }
        Err(e) => {
            panic!("Failed to parse: {:?}", e);
        }
    }
}

#[test]
fn test_flatten_value_empty_table() {
    // Edge case: empty table
    let toml = r#"
[badges.appveyor]
"#;

    let result = facet_toml::from_str::<Config>(toml);
    match &result {
        Ok(config) => {
            println!("Success! Parsed: {:?}", config);
            assert!(config.badges.contains_key("appveyor"));
            let badge = &config.badges["appveyor"];

            // attributes should be an empty object
            assert!(badge.attributes.is_object());
            assert!(badge.attributes.as_object().unwrap().is_empty());
        }
        Err(e) => {
            panic!("Failed to parse empty table with flattened Value: {:?}", e);
        }
    }
}

#[test]
fn test_flatten_value_mixed_fields() {
    // This case has both known and unknown fields
    #[derive(Facet, Debug)]
    pub struct BadgeWithKnown {
        pub repository: Option<String>,
        #[facet(flatten)]
        pub attributes: Value,
    }

    #[derive(Facet, Debug)]
    pub struct ConfigWithKnown {
        pub badges: std::collections::HashMap<String, BadgeWithKnown>,
    }

    let toml = r#"
[badges.appveyor]
repository = "user/repo"
branch = "main"
service = "appveyor"
"#;

    let result = facet_toml::from_str::<ConfigWithKnown>(toml);
    match &result {
        Ok(config) => {
            println!("Success! Parsed: {:?}", config);
            assert!(config.badges.contains_key("appveyor"));
            let badge = &config.badges["appveyor"];

            // repository should be in the known field
            assert_eq!(badge.repository.as_ref().unwrap(), "user/repo");

            // branch and service should be in the flattened attributes
            assert!(badge.attributes.is_object());
            let attrs = badge.attributes.as_object().unwrap();
            assert!(attrs.contains_key("branch"));
            assert!(attrs.contains_key("service"));
        }
        Err(e) => {
            panic!("Failed to parse mixed fields: {:?}", e);
        }
    }
}
