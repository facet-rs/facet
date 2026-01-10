//! Integration tests for facet-default

use facet::Facet;
use facet_default as default;

/// Test struct with various default attributes
#[test]
fn test_struct_default() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(derive(Default))]
    pub struct Config {
        #[facet(default = String::from("localhost"))]
        host: String,
        #[facet(default = 8080u16)]
        port: u16,
        // No attribute = uses Default::default()
        debug: bool,
    }

    let config = Config::default();
    assert_eq!(config.host, "localhost");
    assert_eq!(config.port, 8080);
    assert!(!config.debug);
}

/// Test struct with function defaults
#[test]
fn test_struct_with_func_default() {
    fn default_name() -> String {
        "anonymous".to_string()
    }

    fn default_count() -> usize {
        42
    }

    #[derive(Facet, Debug, PartialEq)]
    #[facet(derive(Default))]
    pub struct User {
        #[facet(default = default_name())]
        name: String,
        #[facet(default = default_count())]
        count: usize,
    }

    let user = User::default();
    assert_eq!(user.name, "anonymous");
    assert_eq!(user.count, 42);
}

/// Test enum with default variant (unit)
#[test]
fn test_enum_default_unit_variant() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(derive(Default))]
    #[repr(u8)]
    pub enum Status {
        #[facet(default::variant)]
        Pending,
        Active,
        Done,
    }

    let status = Status::default();
    assert_eq!(status, Status::Pending);
}

/// Test enum with default variant that has fields
#[test]
fn test_enum_default_tuple_variant() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(derive(Default))]
    #[repr(u8)]
    pub enum Value {
        Empty,
        #[facet(default::variant)]
        Number(#[facet(default = 0)] i32),
        Text(String),
    }

    let value = Value::default();
    assert_eq!(value, Value::Number(0));
}

/// Test enum with struct variant as default
#[test]
fn test_enum_default_struct_variant() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(derive(Default))]
    #[repr(u8)]
    pub enum Request {
        #[facet(default::variant)]
        Get {
            #[facet(default = String::from("/"))]
            path: String,
            #[facet(default = 80u16)]
            port: u16,
        },
        Post {
            path: String,
            body: String,
        },
    }

    let req = Request::default();
    match req {
        Request::Get { path, port } => {
            assert_eq!(path, "/");
            assert_eq!(port, 80);
        }
        _ => panic!("Expected Get variant"),
    }
}

/// Test mixing value and func defaults
#[test]
fn test_mixed_defaults() {
    fn compute_id() -> u64 {
        12345
    }

    #[derive(Facet, Debug)]
    #[facet(derive(Default))]
    pub struct Record {
        #[facet(default = compute_id())]
        id: u64,
        #[facet(default = String::from("untitled"))]
        title: String,
        #[facet(default = true)]
        active: bool,
    }

    let record = Record::default();
    assert_eq!(record.id, 12345);
    assert_eq!(record.title, "untitled");
    assert!(record.active);
}

/// Test builtin #[facet(default = ...)] syntax (issue #1680)
/// This tests that derive(Default) respects field-level default values
/// specified using the builtin syntax rather than the namespaced default::value syntax.
#[test]
fn test_builtin_default_syntax() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(derive(Default))]
    pub struct Config {
        #[facet(default = true)]
        enabled: bool,
        #[facet(default = false)]
        disabled: bool,
        #[facet(default = 42)]
        number: i32,
    }

    let config = Config::default();
    assert!(config.enabled, "enabled should be true");
    assert!(!config.disabled, "disabled should be false");
    assert_eq!(config.number, 42, "number should be 42");
}

/// Test builtin #[facet(default)] without value (uses Default::default())
#[test]
fn test_builtin_default_no_value() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(derive(Default))]
    pub struct Settings {
        #[facet(default)]
        count: usize,
        #[facet(default = 100)]
        limit: usize,
    }

    let settings = Settings::default();
    assert_eq!(settings.count, 0, "count should use Default::default()");
    assert_eq!(settings.limit, 100, "limit should be 100");
}

/// Test for issue #1679: derive(Default) combined with other attributes on the same line
/// Previously, this would fail with "unknown attribute derive" error because the
/// attribute stripping code only checked the first item in a combined attribute.
#[test]
fn test_derive_combined_with_other_attrs() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename_all = "kebab-case", derive(Default))]
    pub struct PreCommitConfig {
        #[facet(default = true)]
        generate_readmes: bool,
        #[facet(default = false)]
        skip_ci: bool,
    }

    let config = PreCommitConfig::default();
    assert!(config.generate_readmes, "generate_readmes should be true");
    assert!(!config.skip_ci, "skip_ci should be false");
}

/// Test derive(Default) with derive first, then other attrs
#[test]
fn test_derive_first_then_other_attrs() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(derive(Default), rename_all = "camelCase")]
    pub struct ApiConfig {
        #[facet(default = 8080)]
        port: u16,
        // Use default without value to test Default::default() for String
        #[facet(default)]
        host: String,
    }

    let config = ApiConfig::default();
    assert_eq!(config.port, 8080);
    assert_eq!(config.host, "");
}
