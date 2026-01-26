//! Tests demonstrating layered configuration from multiple sources:
//! CLI, environment variables, config files, and defaults.

use crate::assert_diag_snapshot;
use facet::Facet;
use figue::{self as args, Driver, MockEnv, builder};

/// A comprehensive server configuration with nested structures.
#[derive(Facet, Debug)]
struct Args {
    /// Enable verbose logging
    #[facet(args::named, args::short = 'v')]
    verbose: bool,

    /// Server configuration (from file/env)
    #[facet(args::config, args::env_prefix = "APP")]
    config: ServerConfig,
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

    /// Database connection settings
    database: DatabaseConfig,

    /// Optional TLS configuration
    #[facet(default)]
    tls: Option<TlsConfig>,
}

/// Database connection configuration.
#[derive(Facet, Debug)]
struct DatabaseConfig {
    /// Database connection URL
    url: String,

    /// Maximum number of connections in the pool
    #[facet(default = 10)]
    max_connections: u32,

    /// Connection timeout in seconds
    #[facet(default = 30)]
    timeout_secs: u64,
}

/// TLS configuration for secure connections.
#[derive(Facet, Debug)]
struct TlsConfig {
    /// Path to the certificate file
    cert_path: String,

    /// Path to the private key file
    key_path: String,
}

#[test]
fn test_layered_all_sources() {
    // Config file content (lowest priority after defaults)
    let config_json = r#"{
        "host": "0.0.0.0",
        "port": 3000,
        "database": {
            "url": "postgres://localhost/mydb",
            "max_connections": 20
        }
    }"#;

    // Environment variables (higher priority than file)
    let env = MockEnv::from_pairs([("APP__PORT", "4000"), ("APP__DATABASE__TIMEOUT_SECS", "60")]);

    // CLI args (highest priority)
    // --verbose is set via CLI
    // --config.host could override but we'll let file win for this field

    let config = builder::<Args>()
        .unwrap()
        .cli(|cli| cli.args(["--verbose"]))
        .env(|e| e.source(env))
        .file(|f| f.content(config_json, "config.json"))
        .build();

    let driver = Driver::new(config);
    let args = driver.run().unwrap();

    // CLI: --verbose
    assert!(args.verbose, "verbose should be true from CLI");
    // File: host = "0.0.0.0"
    assert_eq!(args.config.host, "0.0.0.0", "host should come from file");
    // Env overrides file: port
    assert_eq!(args.config.port, 4000, "port should be overridden by env");
    // File: database.url
    assert_eq!(
        args.config.database.url, "postgres://localhost/mydb",
        "database.url should come from file"
    );
    // File: database.max_connections
    assert_eq!(
        args.config.database.max_connections, 20,
        "max_connections should come from file"
    );
    // Env overrides default: database.timeout_secs
    assert_eq!(
        args.config.database.timeout_secs, 60,
        "timeout_secs should be overridden by env"
    );
    // Default: tls is None
    assert!(args.config.tls.is_none(), "tls should be None (default)");
}

#[test]
fn test_layered_missing_required_field() {
    // Config file missing database.url (required field)
    let config_json = r#"{
        "host": "127.0.0.1",
        "port": 5000,
        "database": {
            "max_connections": 5
        }
    }"#;

    let env = MockEnv::from_pairs([("APP__DATABASE__TIMEOUT_SECS", "15")]);

    let config = builder::<Args>()
        .unwrap()
        .cli(|cli| cli.args(["-v"]))
        .env(|e| e.source(env))
        .file(|f| f.content(config_json, "config.json"))
        .build();

    let driver = Driver::new(config);
    let err = driver.run().unwrap_err();
    assert_diag_snapshot!(err);
}

#[test]
fn test_layered_cli_overrides_all() {
    // File sets everything
    let config_json = r#"{
        "host": "file-host",
        "port": 1111,
        "database": {
            "url": "postgres://file/db",
            "max_connections": 100,
            "timeout_secs": 999
        }
    }"#;

    // Env also sets things
    let env = MockEnv::from_pairs([
        ("APP__HOST", "env-host"),
        ("APP__PORT", "2222"),
        ("APP__DATABASE__URL", "postgres://env/db"),
    ]);

    // CLI overrides host via --config.host
    let config = builder::<Args>()
        .unwrap()
        .cli(|cli| cli.args(["--config.host", "cli-host", "--config.port", "3333"]))
        .env(|e| e.source(env))
        .file(|f| f.content(config_json, "config.json"))
        .build();

    let driver = Driver::new(config);
    let args = driver.run().unwrap();

    // CLI wins for host
    assert_eq!(args.config.host, "cli-host", "host should come from CLI");
    // CLI wins for port
    assert_eq!(args.config.port, 3333, "port should come from CLI");
    // Env wins for database.url (no CLI override)
    assert_eq!(
        args.config.database.url, "postgres://env/db",
        "database.url should come from env"
    );
    // File wins for max_connections (no env or CLI override)
    assert_eq!(
        args.config.database.max_connections, 100,
        "max_connections should come from file"
    );
    // File wins for timeout_secs (no env or CLI override for this one)
    assert_eq!(
        args.config.database.timeout_secs, 999,
        "timeout_secs should come from file"
    );
}

/// Simpler structure for testing dump output with all sources visible.
#[derive(Facet, Debug)]
struct SimpleArgs {
    /// Enable debug mode
    #[facet(args::named, args::short = 'd')]
    debug: bool,

    /// Application settings
    #[facet(args::config, args::env_prefix = "MYAPP")]
    settings: AppSettings,
}

#[derive(Facet, Debug)]
struct AppSettings {
    /// Application name
    name: String,

    /// Server host address
    #[facet(default = "127.0.0.1")]
    host: String,

    /// Server port number
    #[facet(default = 8080)]
    port: u16,

    /// Maximum retry attempts
    #[facet(default = 3)]
    max_retries: u32,

    /// Request timeout in milliseconds
    #[facet(default = 5000)]
    timeout_ms: u64,

    /// Enable experimental features
    #[facet(default)]
    experimental: bool,

    /// Logging configuration
    logging: LogConfig,

    /// Storage backend selection
    storage: StorageBackend,
}

/// Logging configuration with nested options.
#[derive(Facet, Debug)]
struct LogConfig {
    /// Log level (debug, info, warn, error)
    #[facet(default = "info")]
    level: String,

    /// Log output format
    #[facet(default)]
    format: LogFormat,

    /// Optional file output path
    #[facet(default)]
    file: Option<String>,
}

/// Log output format.
#[derive(Facet, Debug, Default)]
#[repr(u8)]
enum LogFormat {
    /// Plain text format
    #[default]
    Plain,
    /// JSON structured logging
    Json,
    /// Compact single-line format
    Compact,
}

/// Storage backend configuration - demonstrates enum with data.
#[derive(Facet, Debug)]
#[facet(rename_all = "kebab-case")]
#[repr(u8)]
#[allow(dead_code)]
enum StorageBackend {
    /// Local filesystem storage
    Local {
        /// Base path for storage
        path: String,
    },
    /// S3-compatible object storage
    S3 {
        /// S3 bucket name
        bucket: String,
        /// AWS region
        #[facet(default = "us-east-1")]
        region: String,
        /// Optional endpoint for S3-compatible services
        #[facet(default)]
        endpoint: Option<String>,
    },
    /// In-memory storage (for testing)
    Memory,
}

#[test]
fn test_layered_dump_shows_all_sources() {
    // This test shows missing required fields with values from multiple sources
    // Demonstrates: nested structs, enums with data, various sources, deep missing fields
    let config_json = r#"{
        "host": "0.0.0.0",
        "port": 3000,
        "max_retries": 5,
        "logging": {
            "level": "debug",
            "format": "Json"
        },
        "storage": {
            "s3": {
                "region": "eu-west-1"
            }
        }
    }"#;

    let env = MockEnv::from_pairs([
        ("MYAPP__PORT", "4000"),                  // overrides file
        ("MYAPP__TIMEOUT_MS", "10000"),           // overrides default
        ("MYAPP__LOGGING__FILE", "/var/log/app"), // sets optional field
    ]);

    let config = builder::<SimpleArgs>()
        .unwrap()
        .cli(|cli| cli.args(["--debug", "--settings.experimental", "true"]))
        .env(|e| e.source(env))
        .file(|f| f.content(config_json, "app.json"))
        .build();

    let driver = Driver::new(config);
    let err = driver.run().unwrap_err();
    // This snapshot should show:
    // - debug: true (from CLI)
    // - name: MISSING (top-level)
    // - host: 0.0.0.0 (from file)
    // - port: 4000 (from env, overriding file's 3000)
    // - max_retries: 5 (from file)
    // - timeout_ms: 10000 (from env, overriding default)
    // - experimental: true (from CLI)
    // - logging.level: debug (from file)
    // - logging.format: Json (from file)
    // - logging.file: /var/log/app (from env)
    // - storage: S3:: (from file)
    //   - bucket: MISSING (deep missing field!)
    //   - region: eu-west-1 (from file)
    //   - endpoint: <default>
    assert_diag_snapshot!(err);
}
