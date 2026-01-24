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

/// Application configuration with layered sources.
#[derive(Facet)]
struct Args {
    /// Show version information.
    #[facet(args::named, args::short = 'v')]
    version: bool,

    /// Verbose output.
    #[facet(args::named, args::counted, args::short = 'V')]
    verbose: u8,
}

/// Main application configuration.
#[derive(Facet)]
struct AppConfig {
    /// Server configuration.
    #[facet(default = "ServerConfig::default()")]
    server: ServerConfig,

    /// Database configuration.
    #[facet(default = "DatabaseConfig::default()")]
    database: DatabaseConfig,

    /// Email configuration (optional).
    email: Option<EmailConfig>,

    /// Feature flags.
    #[facet(default = "FeatureFlags::default()")]
    features: FeatureFlags,
}

/// Server settings.
#[derive(Facet)]
struct ServerConfig {
    /// Server host address.
    #[facet(default = "localhost")]
    host: String,

    /// Server port.
    #[facet(default = 8080)]
    port: u16,

    /// Request timeout in seconds.
    #[facet(default = 30)]
    timeout_secs: u64,

    /// Enable TLS.
    #[facet(default = false)]
    tls_enabled: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "localhost".into(),
            port: 8080,
            timeout_secs: 30,
            tls_enabled: false,
        }
    }
}

/// Database settings.
#[derive(Facet)]
struct DatabaseConfig {
    /// Database URL.
    #[facet(default = "sqlite::memory:")]
    url: String,

    /// Maximum number of connections.
    #[facet(default = 10)]
    max_connections: u32,

    /// Enable query logging.
    #[facet(default = false)]
    log_queries: bool,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: "sqlite::memory:".into(),
            max_connections: 10,
            log_queries: false,
        }
    }
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
}

/// Feature flags for experimental features.
#[derive(Facet)]
struct FeatureFlags {
    /// Enable experimental API.
    #[facet(default = false)]
    experimental_api: bool,

    /// Enable debug mode.
    #[facet(default = false)]
    debug_mode: bool,

    /// Enable metrics collection.
    #[facet(default = true)]
    metrics: bool,
}

impl Default for FeatureFlags {
    fn default() -> Self {
        Self {
            experimental_api: false,
            debug_mode: false,
            metrics: true,
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Layered Configuration Example ===\n");

    // For now, we'll demonstrate the basic parsing since the full layered
    // config integration is still in progress.
    //
    // Once complete, this will automatically:
    // 1. Look for --config <path> flag
    // 2. Load and parse the config file (JSON/YAML/TOML)
    // 3. Parse MYAPP__* environment variables
    // 4. Parse --config.* CLI overrides
    // 5. Merge all layers with correct priority
    // 6. Deserialize into AppConfig

    println!("Current implementation status:");
    println!("✅ ConfigValue enum with provenance tracking");
    println!("✅ Environment variable parsing");
    println!("✅ Config file loading (JSON)");
    println!("✅ Deep-merge of configuration layers");
    println!("✅ CLI override parsing (--config.foo.bar syntax)");
    println!("✅ Type coercion via FormatDeserializer");
    println!("✅ args::config and args::env_prefix attributes");
    println!("🚧 Integration into main parsing flow (next step!)");
    println!();

    // Demonstrate the builder API that's ready to use
    println!("Builder API ready for use:");
    println!();
    println!("  let config: AppConfig = facet_args::builder()");
    println!("      .cli(|cli| cli.args(std::env::args_os().skip(1)))");
    println!("      .env(|env| env.prefix(\"MYAPP\"))");
    println!("      .file(|file| file");
    println!("          .format(facet_args::config_format::JsonFormat)");
    println!("          .default_paths(&[\"config.json\", \"/etc/myapp/config.json\"])");
    println!("      )");
    println!("      .build()?;");
    println!();

    // For demonstration, let's parse basic CLI args
    let args: Args = facet_args::from_std_args()?;

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

    // Show what the configuration structure would look like
    println!("Configuration structure (with defaults):");
    let config = AppConfig {
        server: ServerConfig::default(),
        database: DatabaseConfig::default(),
        email: None,
        features: FeatureFlags::default(),
    };
    println!("{}", config.pretty());
    println!();

    println!("Next steps:");
    println!("- Integrate args::config attribute into parsing flow");
    println!("- Auto-detect config field and load file");
    println!("- Parse env vars with MYAPP__ prefix");
    println!("- Apply CLI overrides with --config.* syntax");
    println!("- Merge layers and deserialize into AppConfig");
    println!();

    println!("Try the builder API manually:");
    println!("  cargo run --example layered_config -- --help");

    Ok(())
}
