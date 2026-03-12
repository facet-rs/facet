//! Tests for untagged enum deserialization in TOML.
//!
//! Untagged enums try each variant in order until one succeeds.

use facet::Facet;

// ============================================================================
// Renamed nested enum variants (issue #1404)
// ============================================================================

#[test]
fn untagged_enum_with_renamed_nested_enum() {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    pub enum Edition {
        #[facet(rename = "2021")]
        E2021,
        #[facet(rename = "2024")]
        E2024,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct WorkspaceRef {
        workspace: bool,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[facet(untagged)]
    pub enum EditionOrWorkspace {
        Edition(Edition),
        Workspace(WorkspaceRef),
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        edition: EditionOrWorkspace,
    }

    // Struct variant
    let toml1 = r#"edition = { workspace = true }"#;
    let config1: Config = facet_toml::from_str(toml1).unwrap();
    assert!(matches!(
        config1.edition,
        EditionOrWorkspace::Workspace(WorkspaceRef { workspace: true })
    ));

    // Enum variant with renamed value
    let toml2 = r#"edition = "2024""#;
    let config2: Config = facet_toml::from_str(toml2).unwrap();
    assert!(matches!(
        config2.edition,
        EditionOrWorkspace::Edition(Edition::E2024)
    ));

    let toml3 = r#"edition = "2021""#;
    let config3: Config = facet_toml::from_str(toml3).unwrap();
    assert!(matches!(
        config3.edition,
        EditionOrWorkspace::Edition(Edition::E2021)
    ));
}

// ============================================================================
// Multiple struct variants (issue #1406)
// ============================================================================

#[test]
fn untagged_enum_with_struct_variants() {
    #[derive(Facet, Debug, Clone, PartialEq)]
    struct DetailedDep {
        version: Option<String>,
        features: Option<Vec<String>>,
    }

    #[derive(Facet, Debug, Clone, PartialEq)]
    struct WorkspaceDep {
        workspace: bool,
    }

    #[derive(Facet, Debug, Clone, PartialEq)]
    #[repr(u8)]
    #[facet(untagged)]
    enum Dep {
        Simple(String),
        Workspace(WorkspaceDep),
        Detailed(DetailedDep),
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        dep: Dep,
    }

    // Simple variant
    let toml1 = r#"dep = "1.0""#;
    let c1: Config = facet_toml::from_str(toml1).unwrap();
    assert_eq!(c1.dep, Dep::Simple("1.0".to_string()));

    // Workspace variant
    let toml2 = r#"dep = { workspace = true }"#;
    let c2: Config = facet_toml::from_str(toml2).unwrap();
    assert_eq!(c2.dep, Dep::Workspace(WorkspaceDep { workspace: true }));

    // Detailed variant
    let toml3 = r#"dep = { version = "2.0", features = ["foo"] }"#;
    let c3: Config = facet_toml::from_str(toml3).unwrap();
    assert_eq!(
        c3.dep,
        Dep::Detailed(DetailedDep {
            version: Some("2.0".to_string()),
            features: Some(vec!["foo".to_string()])
        })
    );
}

// ============================================================================
// Integer coercion in untagged enums (issue #1419)
// ============================================================================

#[derive(Facet, Debug, PartialEq)]
#[repr(u8)]
#[facet(untagged)]
enum DebugLevel {
    Bool(bool),
    Number(u8),
    String(String),
}

#[test]
fn i64_to_u8_coercion_in_untagged_enum() {
    #[derive(Facet, Debug)]
    struct Profile {
        debug: Option<DebugLevel>,
    }

    #[derive(Facet, Debug)]
    struct Manifest {
        profile: Option<std::collections::HashMap<String, Profile>>,
    }

    let toml = r#"
[profile.dev]
debug = 0
"#;

    let manifest: Manifest = facet_toml::from_str(toml).unwrap();
    assert!(manifest.profile.is_some());
    let profile_map = manifest.profile.as_ref().unwrap();
    assert!(profile_map.contains_key("dev"));
    let dev_profile = &profile_map["dev"];
    assert_eq!(dev_profile.debug, Some(DebugLevel::Number(0)));
}

#[test]
fn i64_to_u8_coercion_various_values() {
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
        let config: Config = facet_toml::from_str(&toml).unwrap();
        assert_eq!(config.value, expected, "Failed for value {}", toml_val);
    }
}

#[test]
fn cargo_profile_use_case() {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[facet(untagged)]
    enum OptLevel {
        Number(u8),
        String(String),
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

    let manifest: CargoManifest = facet_toml::from_str(toml).unwrap();
    let profile_map = manifest.profile.as_ref().unwrap();
    let dev_profile = &profile_map["dev"];
    assert_eq!(dev_profile.debug, Some(DebugLevel::Number(0)));
    assert_eq!(dev_profile.opt_level, Some(OptLevel::Number(3)));
}

#[test]
fn i32_coercion_works() {
    #[derive(Facet, Debug)]
    struct LintConfig {
        priority: Option<i32>,
    }

    let config: LintConfig = facet_toml::from_str("priority = -1").unwrap();
    assert_eq!(config.priority, Some(-1));
}
