//! Integration tests for environment variable substitution in config values.

use std::path::PathBuf;

use facet::Facet;
use figue::{self as args, Driver, DriverError};

fn set_env(key: &str, value: &str) {
    // SAFETY: Tests run serially with --test-threads=1 or we use unique env var names
    unsafe { std::env::set_var(key, value) };
}

fn remove_env(key: &str) {
    // SAFETY: Tests run serially with --test-threads=1 or we use unique env var names
    unsafe { std::env::remove_var(key) };
}

// ============================================================================
// Test 1: Basic substitution with ${VAR} when var is set
// ============================================================================

#[derive(Facet, Debug)]
struct ConfigWithEnvSubst {
    #[facet(args::env_subst)]
    data_dir: String,
}

#[derive(Facet, Debug)]
struct ArgsWithEnvSubst {
    #[facet(args::config)]
    config: ConfigWithEnvSubst,
}

#[test]
fn test_basic_substitution_var_set() {
    set_env("FIGUE_TEST_BASE_PATH", "/var/myapp");

    let config = figue::builder::<ArgsWithEnvSubst>()
        .unwrap()
        .file(|f| {
            f.content(
                r#"{"data_dir": "${FIGUE_TEST_BASE_PATH}/data"}"#,
                "config.json",
            )
        })
        .build();

    let result = Driver::new(config).run();
    let output = result.unwrap();
    assert_eq!(output.config.data_dir, "/var/myapp/data");

    remove_env("FIGUE_TEST_BASE_PATH");
}

// ============================================================================
// Test 2: Missing var error - ${VAR} with var not set
// ============================================================================

#[test]
fn test_missing_var_error() {
    // Make sure the var is not set
    remove_env("FIGUE_TEST_NOT_SET");

    let config = figue::builder::<ArgsWithEnvSubst>()
        .unwrap()
        .file(|f| {
            f.content(
                r#"{"data_dir": "${FIGUE_TEST_NOT_SET}/data"}"#,
                "config.json",
            )
        })
        .build();

    let result = Driver::new(config).run();
    let err = result.unwrap_err();

    match err {
        DriverError::EnvSubst { error } => {
            assert_eq!(error.var_name(), "FIGUE_TEST_NOT_SET");
        }
        _ => panic!("Expected EnvSubst error, got {:?}", err),
    }
}

// ============================================================================
// Test 3: Default value - ${VAR:-default} with var not set
// ============================================================================

#[test]
fn test_default_value_used() {
    remove_env("FIGUE_TEST_MAYBE_SET");

    let config = figue::builder::<ArgsWithEnvSubst>()
        .unwrap()
        .file(|f| {
            f.content(
                r#"{"data_dir": "${FIGUE_TEST_MAYBE_SET:-/default/path}"}"#,
                "config.json",
            )
        })
        .build();

    let result = Driver::new(config).run();
    let output = result.unwrap();
    assert_eq!(output.config.data_dir, "/default/path");
}

// ============================================================================
// Test 4: Default value ignored - ${VAR:-default} with var set
// ============================================================================

#[test]
fn test_default_value_ignored_when_set() {
    set_env("FIGUE_TEST_ACTUALLY_SET", "/actual/path");

    let config = figue::builder::<ArgsWithEnvSubst>()
        .unwrap()
        .file(|f| {
            f.content(
                r#"{"data_dir": "${FIGUE_TEST_ACTUALLY_SET:-/default/path}"}"#,
                "config.json",
            )
        })
        .build();

    let result = Driver::new(config).run();
    let output = result.unwrap();
    assert_eq!(output.config.data_dir, "/actual/path");

    remove_env("FIGUE_TEST_ACTUALLY_SET");
}

// ============================================================================
// Test 5: Escape mechanism - $${VAR} produces literal ${VAR}
// ============================================================================

#[test]
fn test_escape_mechanism() {
    let config = figue::builder::<ArgsWithEnvSubst>()
        .unwrap()
        .file(|f| f.content(r#"{"data_dir": "$${LITERAL}"}"#, "config.json"))
        .build();

    let result = Driver::new(config).run();
    let output = result.unwrap();
    assert_eq!(output.config.data_dir, "${LITERAL}");
}

// ============================================================================
// Test 6: Multiple substitutions in one string - ${A}/${B}
// ============================================================================

#[test]
fn test_multiple_substitutions() {
    set_env("FIGUE_TEST_PATH_A", "/var");
    set_env("FIGUE_TEST_PATH_B", "myapp");

    let config = figue::builder::<ArgsWithEnvSubst>()
        .unwrap()
        .file(|f| {
            f.content(
                r#"{"data_dir": "${FIGUE_TEST_PATH_A}/${FIGUE_TEST_PATH_B}"}"#,
                "config.json",
            )
        })
        .build();

    let result = Driver::new(config).run();
    let output = result.unwrap();
    assert_eq!(output.config.data_dir, "/var/myapp");

    remove_env("FIGUE_TEST_PATH_A");
    remove_env("FIGUE_TEST_PATH_B");
}

// ============================================================================
// Test 7: Per-field opt-in - #[facet(args::env_subst)]
// ============================================================================

#[derive(Facet, Debug)]
struct ConfigMixedSubst {
    #[facet(args::env_subst)]
    with_subst: String,
    without_subst: String,
}

#[derive(Facet, Debug)]
struct ArgsMixedSubst {
    #[facet(args::config)]
    config: ConfigMixedSubst,
}

#[test]
fn test_per_field_opt_in() {
    set_env("FIGUE_TEST_SUBST_VAR", "substituted");

    let config = figue::builder::<ArgsMixedSubst>()
        .unwrap()
        .file(|f| {
            f.content(
                r#"{"with_subst": "${FIGUE_TEST_SUBST_VAR}", "without_subst": "${FIGUE_TEST_SUBST_VAR}"}"#,
                "config.json",
            )
        })
        .build();

    let result = Driver::new(config).run();
    let output = result.unwrap();
    // Field with env_subst gets substituted
    assert_eq!(output.config.with_subst, "substituted");
    // Field without env_subst keeps the literal pattern
    assert_eq!(output.config.without_subst, "${FIGUE_TEST_SUBST_VAR}");

    remove_env("FIGUE_TEST_SUBST_VAR");
}

// ============================================================================
// Test 8: Container opt-in - #[facet(args::env_subst_all)]
// ============================================================================

#[derive(Facet, Debug)]
#[facet(args::env_subst_all)]
struct ConfigAllSubst {
    path_a: String,
    path_b: String,
}

#[derive(Facet, Debug)]
struct ArgsAllSubst {
    #[facet(args::config)]
    config: ConfigAllSubst,
}

#[test]
fn test_container_opt_in() {
    set_env("FIGUE_TEST_ALL_BASE", "/base");

    let config = figue::builder::<ArgsAllSubst>()
        .unwrap()
        .file(|f| {
            f.content(
                r#"{"path_a": "${FIGUE_TEST_ALL_BASE}/a", "path_b": "${FIGUE_TEST_ALL_BASE}/b"}"#,
                "config.json",
            )
        })
        .build();

    let result = Driver::new(config).run();
    let output = result.unwrap();
    assert_eq!(output.config.path_a, "/base/a");
    assert_eq!(output.config.path_b, "/base/b");

    remove_env("FIGUE_TEST_ALL_BASE");
}

// ============================================================================
// Test 9: Nested structs - env_subst_all does NOT propagate
// ============================================================================

#[derive(Facet, Debug)]
struct NestedConfig {
    nested_path: String,
}

#[derive(Facet, Debug)]
#[facet(args::env_subst_all)]
struct ConfigWithNested {
    direct_path: String,
    nested: NestedConfig,
}

#[derive(Facet, Debug)]
struct ArgsWithNested {
    #[facet(args::config)]
    config: ConfigWithNested,
}

#[test]
fn test_nested_structs_no_propagate() {
    set_env("FIGUE_TEST_NESTED_BASE", "/base");

    let config = figue::builder::<ArgsWithNested>()
        .unwrap()
        .file(|f| {
            f.content(
                r#"{"direct_path": "${FIGUE_TEST_NESTED_BASE}/direct", "nested": {"nested_path": "${FIGUE_TEST_NESTED_BASE}/nested"}}"#,
                "config.json",
            )
        })
        .build();

    let result = Driver::new(config).run();
    let output = result.unwrap();
    // Direct child gets substitution
    assert_eq!(output.config.direct_path, "/base/direct");
    // Nested struct field does NOT get substitution (env_subst_all only applies to direct children)
    assert_eq!(
        output.config.nested.nested_path,
        "${FIGUE_TEST_NESTED_BASE}/nested"
    );

    remove_env("FIGUE_TEST_NESTED_BASE");
}

// ============================================================================
// Test 10: Flatten - env_subst_all DOES apply to flattened fields
// ============================================================================

#[derive(Facet, Debug)]
struct FlattenedConfig {
    flattened_path: String,
}

#[derive(Facet, Debug)]
#[facet(args::env_subst_all)]
struct ConfigWithFlatten {
    direct_path: String,
    #[facet(flatten)]
    flattened: FlattenedConfig,
}

#[derive(Facet, Debug)]
struct ArgsWithFlatten {
    #[facet(args::config)]
    config: ConfigWithFlatten,
}

#[test]
fn test_flatten_inherits_env_subst_all() {
    set_env("FIGUE_TEST_FLATTEN_BASE", "/base");

    let config = figue::builder::<ArgsWithFlatten>()
        .unwrap()
        .file(|f| {
            f.content(
                r#"{"direct_path": "${FIGUE_TEST_FLATTEN_BASE}/direct", "flattened_path": "${FIGUE_TEST_FLATTEN_BASE}/flattened"}"#,
                "config.json",
            )
        })
        .build();

    let result = Driver::new(config).run();
    let output = result.unwrap();
    // Direct child gets substitution
    assert_eq!(output.config.direct_path, "/base/direct");
    // Flattened field DOES get substitution (flattened fields become direct children)
    assert_eq!(output.config.flattened.flattened_path, "/base/flattened");

    remove_env("FIGUE_TEST_FLATTEN_BASE");
}

// ============================================================================
// Test 11: Non-String types - PathBuf with env_subst works
// ============================================================================

#[derive(Facet, Debug)]
struct ConfigWithPathBuf {
    #[facet(args::env_subst)]
    data_path: PathBuf,
}

#[derive(Facet, Debug)]
struct ArgsWithPathBuf {
    #[facet(args::config)]
    config: ConfigWithPathBuf,
}

#[test]
fn test_pathbuf_substitution() {
    set_env("FIGUE_TEST_PATHBUF_BASE", "/var/data");

    let config = figue::builder::<ArgsWithPathBuf>()
        .unwrap()
        .file(|f| {
            f.content(
                r#"{"data_path": "${FIGUE_TEST_PATHBUF_BASE}/files"}"#,
                "config.json",
            )
        })
        .build();

    let result = Driver::new(config).run();
    let output = result.unwrap();
    assert_eq!(output.config.data_path, PathBuf::from("/var/data/files"));

    remove_env("FIGUE_TEST_PATHBUF_BASE");
}
