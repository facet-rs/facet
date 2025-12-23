// Test for issue #1419: facet-format-toml fails to coerce i64 to u8 in untagged enums
use facet::Facet;

#[derive(Facet, Debug, PartialEq)]
#[repr(u8)]
#[facet(untagged)]
enum DebugLevel {
    Bool(bool),
    Number(u8), // TOML gives i64, needs coercion to u8
    String(String),
}

#[derive(Facet, Debug)]
struct Profile {
    debug: Option<DebugLevel>,
}

#[derive(Facet, Debug)]
struct Manifest {
    profile: Option<std::collections::HashMap<String, Profile>>,
}

#[test]
fn test_i64_to_u8_coercion_in_untagged_enum() {
    // This should work - TOML parses 0 as i64, but it should coerce to u8
    let toml = r#"
[profile.dev]
debug = 0
"#;

    let result = facet_format_toml::from_str::<Manifest>(toml);
    match &result {
        Ok(manifest) => {
            assert!(manifest.profile.is_some());
            let profile_map = manifest.profile.as_ref().unwrap();
            assert!(profile_map.contains_key("dev"));
            let dev_profile = &profile_map["dev"];
            assert!(dev_profile.debug.is_some());
            assert_eq!(dev_profile.debug, Some(DebugLevel::Number(0)));
        }
        Err(e) => {
            panic!("Failed to coerce i64 to u8 in untagged enum: {:?}", e);
        }
    }
}

#[test]
fn test_i64_to_u8_coercion_various_values() {
    // Test various u8-compatible values
    #[derive(Facet, Debug)]
    struct Config {
        value: DebugLevel,
    }

    for (toml_val, expected) in [
        ("0", DebugLevel::Number(0)),
        ("1", DebugLevel::Number(1)),
        ("2", DebugLevel::Number(2)),
        ("255", DebugLevel::Number(255)),
    ] {
        let toml = format!("value = {}", toml_val);
        let result = facet_format_toml::from_str::<Config>(&toml);
        match &result {
            Ok(config) => {
                assert_eq!(config.value, expected, "Failed for value {}", toml_val);
            }
            Err(e) => {
                panic!("Failed to parse value {}: {:?}", toml_val, e);
            }
        }
    }
}

#[test]
fn test_cargo_profile_use_case() {
    // Real-world Cargo.toml profile configuration
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[facet(untagged)]
    enum OptLevel {
        Number(u8),     // 0-3
        String(String), // "s" or "z"
    }

    #[derive(Facet, Debug)]
    struct CargoProfile {
        #[facet(rename = "opt-level")]
        opt_level: Option<OptLevel>,
        debug: Option<DebugLevel>,
    }

    #[derive(Facet, Debug)]
    struct CargoManifest {
        profile: Option<std::collections::HashMap<String, CargoProfile>>,
    }

    let toml = r#"
[profile.dev]
debug = 0
opt-level = 3
"#;

    let result = facet_format_toml::from_str::<CargoManifest>(toml);
    match &result {
        Ok(manifest) => {
            assert!(manifest.profile.is_some());
            let profile_map = manifest.profile.as_ref().unwrap();
            assert!(profile_map.contains_key("dev"));
            let dev_profile = &profile_map["dev"];
            assert_eq!(dev_profile.debug, Some(DebugLevel::Number(0)));
            assert_eq!(dev_profile.opt_level, Some(OptLevel::Number(3)));
        }
        Err(e) => {
            panic!("Failed to parse Cargo.toml profile: {:?}", e);
        }
    }
}

#[test]
fn test_i32_coercion_works() {
    // Verify that i64 → i32 coercion works (as mentioned in the issue)
    #[derive(Facet, Debug)]
    struct LintConfig {
        priority: Option<i32>,
    }

    let result = facet_format_toml::from_str::<LintConfig>("priority = -1");
    assert!(result.is_ok(), "i64 → i32 coercion should work");
    let config = result.unwrap();
    assert_eq!(config.priority, Some(-1));
}
