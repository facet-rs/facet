//! Test for issue #1359: Regression - Untagged enums with Vec variants fail
//!
//! After PR #1358 fixed dotted key handling for untagged enums, Vec<T> variants
//! in untagged enums stopped working with the error:
//! "No tuple-accepting variants in untagged enum VecOrWorkspace"

use facet::Facet;

/// Test 1: Vec variant matching TOML array (BROKEN after PR #1358)
#[test]
fn test_untagged_enum_vec_variant() {
    #[derive(Facet, Debug, Clone, PartialEq)]
    #[repr(u8)]
    #[facet(untagged)]
    pub enum VecOrWorkspace {
        Values(Vec<String>),
        Workspace(WorkspaceRef),
    }

    #[derive(Facet, Debug, Clone, PartialEq)]
    pub struct WorkspaceRef {
        pub workspace: bool,
    }

    #[derive(Facet, Debug, Clone, PartialEq)]
    #[facet(rename_all = "kebab-case")]
    pub struct Package {
        pub authors: Option<VecOrWorkspace>,
    }

    #[derive(Facet, Debug, Clone, PartialEq)]
    pub struct Manifest {
        pub package: Option<Package>,
    }

    // Test Vec variant with array
    let toml = r#"
[package]
authors = ["Alice <alice@example.com>", "Bob <bob@example.com>"]
"#;

    let result = facet_toml::from_str::<Manifest>(toml);
    if let Err(e) = &result {
        eprintln!("Error: {e}");
    }

    assert!(
        result.is_ok(),
        "Should parse TOML array into Vec variant of untagged enum"
    );

    let parsed = result.unwrap();
    assert_eq!(
        parsed.package.as_ref().unwrap().authors,
        Some(VecOrWorkspace::Values(vec![
            "Alice <alice@example.com>".to_string(),
            "Bob <bob@example.com>".to_string()
        ]))
    );

    // Test Workspace variant with dotted key syntax
    let toml2 = r#"
[package]
authors.workspace = true
"#;

    let result2 = facet_toml::from_str::<Manifest>(toml2);
    if let Err(e) = &result2 {
        eprintln!("Error: {e}");
    }

    assert!(
        result2.is_ok(),
        "Should parse dotted key syntax into Workspace variant"
    );

    let parsed2 = result2.unwrap();
    assert_eq!(
        parsed2.package.as_ref().unwrap().authors,
        Some(VecOrWorkspace::Workspace(WorkspaceRef { workspace: true }))
    );
}

/// Test 2: String variant matching TOML string (should still work)
#[test]
fn test_untagged_enum_string_variant() {
    #[derive(Facet, Debug, Clone, PartialEq)]
    #[repr(u8)]
    #[facet(untagged)]
    pub enum StringOrWorkspace {
        String(String),
        Workspace(WorkspaceRef),
    }

    #[derive(Facet, Debug, Clone, PartialEq)]
    pub struct WorkspaceRef {
        pub workspace: bool,
    }

    #[derive(Facet, Debug, Clone, PartialEq)]
    pub struct Package {
        pub version: Option<StringOrWorkspace>,
    }

    // Test String variant
    let toml = r#"
version = "1.0.0"
"#;

    let result = facet_toml::from_str::<Package>(toml);
    assert!(result.is_ok());

    let parsed = result.unwrap();
    assert_eq!(
        parsed.version,
        Some(StringOrWorkspace::String("1.0.0".to_string()))
    );

    // Test Workspace variant
    let toml2 = r#"
version.workspace = true
"#;

    let result2 = facet_toml::from_str::<Package>(toml2);
    assert!(result2.is_ok());

    let parsed2 = result2.unwrap();
    assert_eq!(
        parsed2.version,
        Some(StringOrWorkspace::Workspace(WorkspaceRef {
            workspace: true
        }))
    );
}

/// Test 3: Enum variant matching TOML string
#[test]
fn test_untagged_enum_enum_variant() {
    #[derive(Facet, Debug, Clone, PartialEq)]
    #[repr(u8)]
    #[facet(untagged)]
    pub enum EditionOrWorkspace {
        Workspace(WorkspaceRef),
        Edition(Edition),
    }

    #[derive(Facet, Debug, Clone, PartialEq)]
    pub struct WorkspaceRef {
        pub workspace: bool,
    }

    #[derive(Facet, Debug, Clone, PartialEq)]
    #[repr(u8)]
    pub enum Edition {
        #[facet(rename = "2015")]
        E2015,
        #[facet(rename = "2018")]
        E2018,
        #[facet(rename = "2021")]
        E2021,
        #[facet(rename = "2024")]
        E2024,
    }

    #[derive(Facet, Debug, Clone, PartialEq)]
    pub struct Package {
        pub edition: Option<EditionOrWorkspace>,
    }

    // Test Edition variant
    let toml = r#"
edition = "2024"
"#;

    let result = facet_toml::from_str::<Package>(toml);
    if let Err(e) = &result {
        eprintln!("Error: {e}");
    }
    assert!(result.is_ok());

    let parsed = result.unwrap();
    assert_eq!(
        parsed.edition,
        Some(EditionOrWorkspace::Edition(Edition::E2024))
    );

    // Test Workspace variant
    let toml2 = r#"
edition.workspace = true
"#;

    let result2 = facet_toml::from_str::<Package>(toml2);
    assert!(result2.is_ok());

    let parsed2 = result2.unwrap();
    assert_eq!(
        parsed2.edition,
        Some(EditionOrWorkspace::Workspace(WorkspaceRef {
            workspace: true
        }))
    );
}

/// Test 4: BoolOrVec pattern
#[test]
fn test_untagged_enum_bool_or_vec() {
    #[derive(Facet, Debug, Clone, PartialEq)]
    #[repr(u8)]
    #[facet(untagged)]
    pub enum BoolOrVec {
        Bool(bool),
        Vec(Vec<String>),
    }

    #[derive(Facet, Debug, Clone, PartialEq)]
    pub struct Package {
        pub publish: Option<BoolOrVec>,
    }

    // Test Bool variant
    let toml = r#"
publish = false
"#;

    let result = facet_toml::from_str::<Package>(toml);
    assert!(result.is_ok());

    let parsed = result.unwrap();
    assert_eq!(parsed.publish, Some(BoolOrVec::Bool(false)));

    // Test Vec variant
    let toml2 = r#"
publish = ["crates-io", "my-registry"]
"#;

    let result2 = facet_toml::from_str::<Package>(toml2);
    if let Err(e) = &result2 {
        eprintln!("Error: {e}");
    }
    assert!(result2.is_ok());

    let parsed2 = result2.unwrap();
    assert_eq!(
        parsed2.publish,
        Some(BoolOrVec::Vec(vec![
            "crates-io".to_string(),
            "my-registry".to_string()
        ]))
    );
}
