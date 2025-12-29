//! Regression test for issue #1408: transparent deserialization with #[facet(metadata = span)]

use facet::Facet;
use facet_reflect::Spanned;

#[test]
fn test_spanned_string() {
    #[derive(Facet, Debug)]
    struct Config {
        name: Spanned<String>,
    }

    let toml = r#"
name = "foo"
"#;

    let config: Config = facet_toml::from_str(toml).unwrap();
    assert_eq!(config.name.value, "foo");
    // Span should be default (unknown) since most format parsers don't track spans
    assert!(config.name.span.is_unknown());
}

#[test]
fn test_spanned_vec() {
    #[derive(Facet, Debug)]
    struct Config {
        features: Spanned<Vec<String>>,
    }

    let toml = r#"
features = ["a", "b", "c"]
"#;

    let config: Config = facet_toml::from_str(toml).unwrap();
    assert_eq!(config.features.value, vec!["a", "b", "c"]);
    assert!(config.features.span.is_unknown());
}

#[test]
fn test_spanned_bool() {
    #[derive(Facet, Debug)]
    struct Config {
        enabled: Spanned<bool>,
    }

    let toml = r#"
enabled = true
"#;

    let config: Config = facet_toml::from_str(toml).unwrap();
    assert!(config.enabled.value);
    assert!(config.enabled.span.is_unknown());
}

#[test]
fn test_multiple_spanned_fields() {
    #[derive(Facet, Debug)]
    struct Config {
        name: Spanned<String>,
        features: Spanned<Vec<String>>,
        enabled: Spanned<bool>,
    }

    let toml = r#"
name = "foo"
features = ["a", "b", "c"]
enabled = true
"#;

    let config: Config = facet_toml::from_str(toml).unwrap();
    assert_eq!(config.name.value, "foo");
    assert_eq!(config.features.value, vec!["a", "b", "c"]);
    assert!(config.enabled.value);

    assert!(config.name.span.is_unknown());
    assert!(config.features.span.is_unknown());
    assert!(config.enabled.span.is_unknown());
}

#[test]
fn test_spanned_integer() {
    #[derive(Facet, Debug)]
    struct Config {
        version: Spanned<u32>,
    }

    let toml = r#"
version = 42
"#;

    let config: Config = facet_toml::from_str(toml).unwrap();
    assert_eq!(config.version.value, 42);
    assert!(config.version.span.is_unknown());
}

#[test]
fn test_nested_spanned_in_table() {
    #[derive(Facet, Debug)]
    struct Config {
        dependency: Dependency,
    }

    #[derive(Facet, Debug)]
    struct Dependency {
        git: Option<Spanned<String>>,
        features: Option<Spanned<Vec<String>>>,
        default_features: Option<Spanned<bool>>,
    }

    let toml = r#"
[dependency]
git = "https://github.com/user/repo"
features = ["a", "b"]
default_features = false
"#;

    let config: Config = facet_toml::from_str(toml).unwrap();
    assert_eq!(
        config.dependency.git.as_ref().unwrap().value,
        "https://github.com/user/repo"
    );
    assert_eq!(
        config.dependency.features.as_ref().unwrap().value,
        vec!["a", "b"]
    );
    assert!(!config.dependency.default_features.as_ref().unwrap().value);
}

#[test]
fn test_spanned_in_array_of_tables() {
    #[derive(Facet, Debug)]
    struct Config {
        dependencies: Vec<Dependency>,
    }

    #[derive(Facet, Debug)]
    struct Dependency {
        name: Spanned<String>,
        version: Spanned<String>,
    }

    let toml = r#"
[[dependencies]]
name = "foo"
version = "1.0"

[[dependencies]]
name = "bar"
version = "2.0"
"#;

    let config: Config = facet_toml::from_str(toml).unwrap();
    assert_eq!(config.dependencies.len(), 2);
    assert_eq!(config.dependencies[0].name.value, "foo");
    assert_eq!(config.dependencies[0].version.value, "1.0");
    assert_eq!(config.dependencies[1].name.value, "bar");
    assert_eq!(config.dependencies[1].version.value, "2.0");
}
