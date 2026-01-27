//! Tests for unknown configuration key handling.
//!
//! These tests verify that:
//! 1. Unknown keys produce helpful error messages (showing valid keys)
//! 2. --help works even when there are unknown keys in config/env
//! 3. `--config /path/to/file` is NOT silently ignored
//!
//! Bug report scenario:
//! - User has env vars like APP__AUTH__EMAIL_FROM set
//! - User runs `myapp --help`
//! - Expected: help text
//! - Actual: "Error: unknown configuration key: auth.email_from" (x8)

use facet::Facet;
use figue::{self as args, Driver, FigueBuiltins, MockEnv, builder};

/// Server configuration for testing unknown keys.
#[derive(Facet, Debug)]
struct Args {
    /// Server configuration (from file/env)
    #[facet(args::config, args::env_prefix = "APP")]
    config: ServerConfig,

    /// Standard CLI options
    #[facet(flatten)]
    builtins: FigueBuiltins,
}

/// Server configuration loaded from config file or environment.
#[derive(Facet, Debug)]
struct ServerConfig {
    /// The host to bind to
    #[facet(default = "localhost")]
    host: String,

    /// The port to listen on
    #[facet(default = 8080)]
    port: u16,

    /// Magic link URL for auth
    #[facet(default)]
    magic_link: Option<String>,
}

// ============================================================================
// Bug #2: --help should work even with unknown keys (env strict mode)
// This is the exact scenario from the bug report
// ============================================================================

#[test]
fn test_help_works_with_unknown_env_keys_strict() {
    // User has unknown env vars set AND passes --help
    // This is the exact scenario from the bug report:
    // - env vars like APP__AUTH__EMAIL_FROM are set (unknown keys)
    // - user runs `reef --help`
    // - expected: help text
    // - actual (bug): wall of "Error: unknown configuration key" messages
    let env = MockEnv::from_pairs([
        ("APP__AUTH__EMAIL_FROM", "noreply@example.com"),
        ("APP__AUTH__MAGIC_LINK_BASE_URL", "https://example.com"),
        ("APP__AUTH__RP_ID", "example.com"),
        ("APP__AUTH__RP_NAME", "Example"),
        ("APP__AUTH__RP_ORIGIN", "https://example.com"),
        ("APP__AUTH__SMTP_HOST", "smtp.example.com"),
        ("APP__AUTH__SMTP_PASSWORD", "secret"),
        ("APP__AUTH__SMTP_USERNAME", "user"),
    ]);

    let config = builder::<Args>()
        .unwrap()
        .cli(|cli| cli.args(["--help"]).strict())
        .env(|e| e.source(env).strict())
        .build();

    let driver = Driver::new(config);
    let result = driver.run();

    // Bug #2: This should return Help, not Failed
    assert!(
        result.is_err(),
        "should be an error (Help is returned as error)"
    );
    let err = result.unwrap_err();
    assert!(
        err.is_help(),
        "should be a Help error, not a parsing error. Got: {}",
        err
    );

    let help = err.help_text().expect("should have help text");
    assert!(help.contains("--help"), "help text should contain --help");
}

#[test]
fn test_help_short_flag_works_with_unknown_env_keys_strict() {
    // Same test but with -h instead of --help
    let env = MockEnv::from_pairs([("APP__UNKNOWN_FIELD", "value")]);

    let config = builder::<Args>()
        .unwrap()
        .cli(|cli| cli.args(["-h"]).strict())
        .env(|e| e.source(env).strict())
        .build();

    let driver = Driver::new(config);
    let result = driver.run();

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.is_help(),
        "should be a Help error with -h flag. Got: {}",
        err
    );
}

#[test]
fn test_help_works_with_unknown_file_keys_strict() {
    // Config file has unknown keys AND user passes --help
    let config_json = r#"{
        "auth": {
            "email_from": "noreply@example.com"
        }
    }"#;

    let config = builder::<Args>()
        .unwrap()
        .cli(|cli| cli.args(["--help"]).strict())
        .file(|f| f.content(config_json, "config.json").strict())
        .build();

    let driver = Driver::new(config);
    let result = driver.run();

    // Bug #2: This should return Help, not Failed
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.is_help(),
        "should be a Help error, not a parsing error. Got: {}",
        err
    );
}

// ============================================================================
// Bug #1: Unknown keys should produce helpful error messages
// ============================================================================

#[test]
fn test_unknown_key_in_env_strict_mode_shows_valid_fields() {
    // User sets an unknown env var (APP__AUTH__EMAIL_FROM doesn't exist)
    // Error should tell them what fields ARE valid, not just say "unknown key"
    let env = MockEnv::from_pairs([
        ("APP__AUTH__EMAIL_FROM", "noreply@example.com"),
        ("APP__AUTH__SMTP_HOST", "smtp.example.com"),
    ]);

    let config = builder::<Args>()
        .unwrap()
        .cli(|cli| cli.args::<[&str; 0], _>([]).strict())
        .env(|e| e.source(env).strict())
        .build();

    let driver = Driver::new(config);
    let err = driver.run().unwrap_err();
    let err_str = err.to_string();

    // The error should mention the unknown key
    assert!(
        err_str.contains("auth.email_from")
            || err_str.contains("AUTH__EMAIL_FROM")
            || err_str.contains("unknown"),
        "error should mention the unknown key, got: {}",
        err_str
    );

    // Bug #1: The error should show what fields ARE valid
    // Currently it just says "unknown configuration key: auth.email_from"
    // It should list valid fields like: host, port, magic_link
    // OR at minimum show the user they can run --help for more info
    assert!(
        err_str.contains("host")
            || err_str.contains("port")
            || err_str.contains("magic_link")
            || err_str.contains("--help")
            || err_str.contains("Valid"),
        "error should show valid fields or suggest --help, got: {}",
        err_str
    );
}

#[test]
fn test_unknown_key_in_file_strict_mode_shows_valid_fields() {
    // Config file has unknown keys
    let config_json = r#"{
        "auth": {
            "email_from": "noreply@example.com",
            "rp_id": "example.com"
        }
    }"#;

    let config = builder::<Args>()
        .unwrap()
        .cli(|cli| cli.args::<[&str; 0], _>([]).strict())
        .file(|f| f.content(config_json, "config.json").strict())
        .build();

    let driver = Driver::new(config);
    let err = driver.run().unwrap_err();
    let err_str = err.to_string();

    // Error should show valid fields via dump, suggest --help
    assert!(
        err_str.contains("host")
            || err_str.contains("port")
            || err_str.contains("magic_link")
            || err_str.contains("--help")
            || err_str.contains("Valid"),
        "error should show valid fields or suggest --help, got: {}",
        err_str
    );
}

// ============================================================================
// Bug #3: --<config-field-name> <path> should not be silently ignored
//
// The CLI flag name comes from the effective name of the field with args::config.
// - Field named `config` -> `--config <path>`
// - Field named `settings` -> `--settings <path>`
// - Field with `#[facet(rename = "cfg")]` -> `--cfg <path>`
// ============================================================================

/// Args with a config field named "config" -> expects --config <path>
#[derive(Facet, Debug)]
struct ArgsWithConfigField {
    #[facet(args::config, args::env_prefix = "APP")]
    config: ServerConfig,

    #[facet(flatten)]
    builtins: FigueBuiltins,
}

/// Args with a config field named "settings" -> expects --settings <path>
#[derive(Facet, Debug)]
struct ArgsWithSettingsField {
    #[facet(args::config, args::env_prefix = "APP")]
    settings: ServerConfig,

    #[facet(flatten)]
    builtins: FigueBuiltins,
}

/// Args with a renamed config field -> expects --cfg <path>
#[derive(Facet, Debug)]
struct ArgsWithRenamedConfigField {
    #[facet(args::config, args::env_prefix = "APP", rename = "cfg")]
    config: ServerConfig,

    #[facet(flatten)]
    builtins: FigueBuiltins,
}

#[test]
fn test_config_path_flag_not_silently_ignored() {
    // User passes --config /path/to/file.json
    // This should NOT be silently ignored - it should either:
    // 1. Load the file and use it as config, OR
    // 2. Report an error (unknown flag, file not found, etc.)
    //
    // Currently it's silently ignored, which is the bug.
    let config = builder::<ArgsWithConfigField>()
        .unwrap()
        .cli(|cli| cli.args(["--config", "/etc/app/config.json"]).strict())
        .build();

    let driver = Driver::new(config);
    let result = driver.run();

    // The result should NOT silently succeed with defaults
    // It should either use the file or error
    match result.into_result() {
        Ok(output) => {
            // If it succeeds, it means the file was loaded (or all fields have defaults)
            // For this test, we want to verify the flag wasn't ignored
            // Since we didn't set up a file layer, this is actually a bug
            panic!(
                "--config flag was silently ignored! Got success with defaults: {:?}",
                output.value
            );
        }
        Err(err) => {
            // An error is acceptable - it means the flag was recognized
            // (e.g., "file not found" or "unknown flag" would both be fine)
            let err_str = err.to_string();
            // But it should NOT be "unknown flag: --config" in strict mode
            // because --config should be a recognized flag for the config field
            assert!(
                !err_str.contains("unknown flag: --config"),
                "--config should be recognized as the config file path flag, got: {}",
                err_str
            );
        }
    }
}

#[test]
fn test_settings_path_flag_not_silently_ignored() {
    // Same test but with field named "settings" -> --settings <path>
    let config = builder::<ArgsWithSettingsField>()
        .unwrap()
        .cli(|cli| cli.args(["--settings", "/etc/app/settings.json"]).strict())
        .build();

    let driver = Driver::new(config);
    let result = driver.run();

    match result.into_result() {
        Ok(output) => {
            panic!(
                "--settings flag was silently ignored! Got success with defaults: {:?}",
                output.value
            );
        }
        Err(err) => {
            let err_str = err.to_string();
            assert!(
                !err_str.contains("unknown flag: --settings"),
                "--settings should be recognized as the config file path flag, got: {}",
                err_str
            );
        }
    }
}

#[test]
fn test_renamed_config_path_flag_not_silently_ignored() {
    // Same test but with renamed field -> --cfg <path>
    let config = builder::<ArgsWithRenamedConfigField>()
        .unwrap()
        .cli(|cli| cli.args(["--cfg", "/etc/app/config.json"]).strict())
        .build();

    let driver = Driver::new(config);
    let result = driver.run();

    match result.into_result() {
        Ok(output) => {
            panic!(
                "--cfg flag was silently ignored! Got success with defaults: {:?}",
                output.value
            );
        }
        Err(err) => {
            let err_str = err.to_string();
            assert!(
                !err_str.contains("unknown flag: --cfg"),
                "--cfg should be recognized as the config file path flag, got: {}",
                err_str
            );
        }
    }
}
