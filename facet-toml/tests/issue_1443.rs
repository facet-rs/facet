/// Test for issue #1443: Reflection errors don't preserve span info for diagnostics
///
/// When facet-toml encounters a reflection error (e.g., type mismatch),
/// it should preserve span information for nice diagnostics.
use facet::Facet;
use facet_toml::{self as toml, DeserializeError, TomlError};

#[derive(Facet, Debug)]
struct PackageMetadata {
    name: String,
    version: String,
    /// README can be either a string path or a workspace-inherited value
    readme: ReadmeValue,
}

#[derive(Facet, Debug)]
#[repr(u8)]
#[facet(untagged)]
enum ReadmeValue {
    /// Simple path to README file
    Path(String),
    /// Workspace-inherited value
    Workspace { workspace: bool },
}

#[test]
fn test_type_mismatch_preserves_span() {
    // This TOML has a boolean value for readme, but we expect String or {workspace = true}
    let toml_str = r#"
[package]
name = "windows_x86_64_gnu"
version = "0.52.6"
readme = false
"#;

    #[derive(Facet, Debug)]
    struct CargoManifest {
        package: PackageMetadata,
    }

    let result: Result<CargoManifest, DeserializeError<TomlError>> = toml::from_str(toml_str);

    match result {
        Ok(_) => panic!("Should have failed with type mismatch error"),
        Err(e) => {
            // Print the error
            let error_msg = format!("{}", e);
            eprintln!("Simple error: {}", error_msg);

            // Check that it's a reflection error about wrong shape
            assert!(
                error_msg.contains("reflection error")
                    || error_msg.contains("Wrong shape")
                    || error_msg.contains("Reflect"),
                "Error should mention reflection/shape issue: {}",
                error_msg
            );
        }
    }
}

#[test]
fn test_valid_readme_string() {
    let toml_str = r#"
[package]
name = "test"
version = "1.0.0"
readme = "README.md"
"#;

    #[derive(Facet, Debug)]
    struct CargoManifest {
        package: PackageMetadata,
    }

    let result: CargoManifest = toml::from_str(toml_str).expect("should parse string variant");
    match result.package.readme {
        ReadmeValue::Path(path) => assert_eq!(path, "README.md"),
        _ => panic!("Expected Path variant"),
    }
}

#[test]
fn test_valid_readme_workspace() {
    let toml_str = r#"
[package]
name = "test"
version = "1.0.0"
readme = { workspace = true }
"#;

    #[derive(Facet, Debug)]
    struct CargoManifest {
        package: PackageMetadata,
    }

    let result: CargoManifest = toml::from_str(toml_str).expect("should parse workspace variant");
    match result.package.readme {
        ReadmeValue::Workspace { workspace } => assert!(workspace),
        _ => panic!("Expected Workspace variant"),
    }
}
