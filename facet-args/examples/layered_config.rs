//! Comprehensive example showcasing the layered configuration system.
//!
//! This example demonstrates:
//! - Config file loading (JSON format)
//! - Environment variable overrides
//! - CLI argument overrides
//! - Nested configuration structures
//! - Default values
//!
//! Run with different configurations:
//!
//! ```bash
//! # Using defaults
//! cargo run --example layered_config
//!
//! # With config file
//! cargo run --example layered_config -- --config examples/config.json
//!
//! # With env vars
//! MYAPP__SERVER__PORT=9000 cargo run --example layered_config
//!
//! # With CLI overrides
//! cargo run --example layered_config -- --config.server.port 8080
//!
//! # Combined (priority: CLI > env > file > defaults)
//! MYAPP__SERVER__HOST=example.com cargo run --example layered_config -- \
//!   --config examples/config.json --config.server.port 3000
//! ```

use facet::Facet;
use facet_args as args;
use facet_pretty::FacetPretty;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

/// Application configuration with layered sources.
#[derive(Facet)]
#[facet(derive(Default), traits(Default))]
struct Args {
    /// Show version information.
    #[facet(args::named, args::short = 'v')]
    version: bool,

    /// Verbose output.
    #[facet(args::named, args::counted, args::short = 'V')]
    verbose: u8,

    /// Dump the final merged configuration and exit.
    #[facet(args::named)]
    dump_config: bool,

    /// Application settings loaded from multiple sources.
    #[facet(args::config, args::env_prefix = "MYAPP")]
    settings: AppConfig,
}

/// Main application configuration.
#[derive(Facet)]
#[facet(derive(Default), traits(Default))]
struct AppConfig {
    /// Server configuration.
    #[facet(default)]
    server: ServerConfig,

    /// Database configuration.
    #[facet(default)]
    database: DatabaseConfig,

    /// Email configuration (optional).
    email: Option<EmailConfig>,

    /// Feature flags.
    #[facet(default)]
    features: FeatureFlags,

    /// List of allowed admin email addresses.
    /// Set via env: MYAPP__ALLOWED_ADMINS=alice@example.com,bob@example.com
    allowed_admins: Option<Vec<String>>,
}

/// Server settings.
#[derive(Facet)]
#[facet(derive(Default), traits(Default))]
struct ServerConfig {
    /// Server host address.
    #[facet(default = "localhost".to_string())]
    host: String,

    /// Server port.
    #[facet(default = 8080)]
    port: u16,

    /// Request timeout in seconds.
    #[facet(default = 30)]
    timeout_secs: u64,

    /// Enable TLS.
    tls_enabled: bool,

    /// API key for authentication (sensitive).
    #[facet(sensitive)]
    api_key: Option<String>,
}

/// Database settings.
#[derive(Facet)]
#[facet(derive(Default), traits(Default))]
struct DatabaseConfig {
    /// Database URL.
    #[facet(default = "sqlite::memory:".to_string())]
    url: String,

    /// Maximum number of connections.
    #[facet(default = 10)]
    max_connections: u32,

    /// Enable query logging.
    log_queries: bool,
}

/// Email/SMTP configuration.
#[derive(Facet)]
struct EmailConfig {
    /// SMTP host.
    host: String,

    /// SMTP port.
    #[facet(default = 587)]
    port: u16,

    /// SMTP username.
    username: Option<String>,

    /// SMTP password.
    password: Option<String>,

    /// From address.
    from: String,

    /// Email footer text (can be very long).
    footer: Option<String>,

    /// Welcome message (can have newlines).
    welcome_message: Option<String>,
}

/// Feature flags for experimental features.
#[derive(Facet)]
#[facet(derive(Default), traits(Default))]
struct FeatureFlags {
    /// Enable experimental API.
    experimental_api: bool,

    /// Enable debug mode.
    debug_mode: bool,

    /// Enable metrics collection.
    #[facet(default = true)]
    metrics: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Set up tracing - use RUST_LOG env var to control verbosity
    // e.g., RUST_LOG=debug cargo run --example layered_config
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    println!("=== Layered Configuration Example ===\n");

    let args_vec: Vec<String> = std::env::args().skip(1).collect();
    let args_str: Vec<&str> = args_vec.iter().map(|s| s.as_str()).collect();

    // Check if --config was explicitly provided
    let explicit_config = args_str
        .iter()
        .position(|arg| *arg == "--config")
        .and_then(|i| args_str.get(i + 1).map(|s| s.to_string()))
        .or_else(|| {
            args_str
                .iter()
                .find(|arg| arg.starts_with("--config="))
                .and_then(|arg| arg.strip_prefix("--config=").map(|s| s.to_string()))
        });

    // Use builder API to demonstrate default config path resolution
    use facet_args::builder;
    use facet_args::config_format::JsonFormat;

    let result = builder()
        .cli(|cli| cli.args(args_str.iter().map(|s| s.to_string())))
        .env(|env| env.prefix("MYAPP"))
        .file(|file| {
            let mut file_builder = file.format(JsonFormat).default_paths(&[
                "./config.json",
                "~/.config/myapp/config.json",
                "facet-args/examples/config.json",
                "/etc/myapp/config.json",
            ]);
            if let Some(path) = explicit_config {
                file_builder = file_builder.path(path);
            }
            file_builder
        })
        .build_traced()?;

    // Check if --dump-config was requested
    if args_vec.iter().any(|arg| arg == "--dump-config") {
        facet_args::dump_config_from_result::<Args>(&result);
        return Ok(());
    }

    // Deserialize the config value into Args
    let args: Args = facet_args::config_value_parser::from_config_value(&result.value)?;

    if args.version {
        println!("myapp v1.0.0");
        return Ok(());
    }

    let verbosity = match args.verbose {
        0 => "normal",
        1 => "verbose",
        2 => "very verbose",
        _ => "debug",
    };
    println!("Verbosity: {}", verbosity);
    println!();

    println!("Loaded configuration:");
    println!("{}", args.settings.pretty());
    println!();

    // Demonstrate that sensitive fields are redacted
    if args.settings.server.api_key.is_some() {
        println!("🔒 API key is set (value hidden due to #[facet(sensitive)])");
        println!();
    }

    println!("✅ Layered configuration is now working!");
    println!();
    println!("Try it out:");
    println!("  # Override with env vars:");
    println!("  MYAPP__SERVER__PORT=9000 cargo run --example layered_config");
    println!();
    println!("  # Override with CLI:");
    println!("  cargo run --example layered_config -- --settings.server.port 3000");
    println!();
    println!("  # Load from config file:");
    println!("  cargo run --example layered_config -- --config facet-args/examples/config.json");
    println!();
    println!("  # Dump final merged config:");
    println!(
        "  cargo run --example layered_config -- --config facet-args/examples/config.json --dump-config"
    );
    println!();
    println!("  # Combine all layers (priority: CLI > env > file > defaults):");
    println!("  MYAPP__SERVER__HOST=example.com cargo run --example layered_config -- \\");
    println!("    --config facet-args/examples/config.json --settings.server.port 4000");
    println!();
    println!("  # Test Vec/List handling with comma-separated env vars:");
    println!("  MYAPP__ALLOWED_ADMINS=alice@example.com,bob@example.com,charlie@example.com \\");
    println!("    cargo run --example layered_config -- --dump-config");

    Ok(())
}
