//! Regression tests for issues #1728 and #1729: null values and comment-only YAML
//!
//! #1728: Null values should use defaults for structs with #[facet(default)]
//! #1729: Comment-only YAML files cause UnexpectedEof error
//!
//! See: https://github.com/facet-rs/facet/issues/1728
//! See: https://github.com/facet-rs/facet/issues/1729

use facet::Facet;
use facet_default as _;

#[derive(Debug, Facet, PartialEq)]
#[facet(rename_all = "kebab-case", derive(Default), traits(Default))]
struct PreCommitConfig {
    #[facet(default = true)]
    generate_readmes: bool,
    #[facet(default = true)]
    rustfmt: bool,
}

#[derive(Debug, Facet, PartialEq)]
#[facet(rename_all = "kebab-case", derive(Default), traits(Default))]
struct CaptainConfig {
    #[facet(default)]
    pre_commit: PreCommitConfig,
}

/// Issue #1728: When YAML parses a key with only comments underneath as `null`,
/// facet-yaml should use the struct's default value instead of throwing a TypeMismatch error.
#[test]
fn test_issue_1728_null_struct_uses_default() {
    let yaml = r#"
# Captain configuration
pre-commit:
  # generate-readmes: false
  # rustfmt: false
"#;
    let config: CaptainConfig = facet_yaml::from_str(yaml).expect("should deserialize");
    assert!(config.pre_commit.generate_readmes);
    assert!(config.pre_commit.rustfmt);
}

/// Issue #1729: When a YAML file contains only comments (no values),
/// facet-yaml should use defaults for the root struct instead of throwing an UnexpectedEof error.
#[test]
fn test_issue_1729_comment_only_yaml_uses_defaults() {
    let yaml = r#"
# Captain configuration
# This file is intentionally empty - all defaults apply
"#;
    let config: CaptainConfig = facet_yaml::from_str(yaml).expect("should deserialize");
    assert!(config.pre_commit.generate_readmes);
    assert!(config.pre_commit.rustfmt);
}

/// Test that empty string works (baseline for #1729)
#[test]
fn test_empty_string_uses_defaults() {
    let yaml = "";
    let config: CaptainConfig = facet_yaml::from_str(yaml).expect("should deserialize");
    assert!(config.pre_commit.generate_readmes);
    assert!(config.pre_commit.rustfmt);
}

/// Test that explicit null at root level uses defaults
#[test]
fn test_explicit_null_at_root_uses_defaults() {
    let yaml = "~";
    let config: CaptainConfig = facet_yaml::from_str(yaml).expect("should deserialize");
    assert!(config.pre_commit.generate_readmes);
    assert!(config.pre_commit.rustfmt);
}

/// Test that explicit null for nested struct uses defaults
#[test]
fn test_explicit_null_for_nested_struct_uses_defaults() {
    let yaml = "pre-commit: null";
    let config: CaptainConfig = facet_yaml::from_str(yaml).expect("should deserialize");
    assert!(config.pre_commit.generate_readmes);
    assert!(config.pre_commit.rustfmt);
}

/// Test partial struct with some fields commented out
#[test]
fn test_partial_struct_with_defaults() {
    let yaml = r#"
pre-commit:
  generate-readmes: false
  # rustfmt: false  <- should default to true
"#;
    let config: CaptainConfig = facet_yaml::from_str(yaml).expect("should deserialize");
    assert!(!config.pre_commit.generate_readmes);
    assert!(config.pre_commit.rustfmt);
}
