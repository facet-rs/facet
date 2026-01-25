//! Demonstration of missing required configuration fields error reporting.
//!
//! This example shows how facet-args handles missing required configuration
//! with helpful inline markers and instructions for how to set them.
//!
//! Run with different scenarios:
//!
//! ```bash
//! # Missing all required fields
//! cargo run --example demo_missing_config
//!
//! # Set one field via CLI
//! cargo run --example demo_missing_config -- --config.email.host smtp.example.com
//!
//! # Set via environment variables
//! MYAPP__CONFIG__EMAIL__HOST=smtp.example.com \
//! MYAPP__CONFIG__EMAIL__FROM=sender@example.com \
//!   cargo run --example demo_missing_config
//!
//! # Set all required fields
//! cargo run --example demo_missing_config -- \
//!   --config.email.host smtp.example.com \
//!   --config.email.from sender@example.com
//! ```

use facet::Facet;
use facet_args as args;

/// Application with required configuration.
#[derive(Facet)]
struct Args {
    /// Application configuration.
    #[facet(args::config, args::env_prefix = "MYAPP")]
    config: AppConfig,
}

/// Application configuration with required fields.
#[derive(Facet)]
struct AppConfig {
    /// Email configuration (REQUIRED - not optional).
    email: EmailConfig,
}

/// Email/SMTP configuration with required fields.
#[derive(Facet)]
struct EmailConfig {
    /// SMTP host
    ///
    /// The hostname or IP address of your SMTP server.
    host: String,

    /// SMTP port.
    #[facet(default = 25)]
    port: u16,

    /// SMTP username
    username: Option<String>,

    /// SMTP password
    #[facet(sensitive)]
    password: Option<String>,

    /// From address
    ///
    /// The email address that will appear in the "From" field.
    from: String,
}

fn main() {
    println!("=== Missing Required Configuration Demo ===\n");

    let args_vec: Vec<String> = std::env::args().skip(1).collect();
    let args_str: Vec<&str> = args_vec.iter().map(|s| s.as_str()).collect();

    // This will show a helpful error if required fields are missing
    let result = match facet_args::from_slice_layered::<Args>(&args_str) {
        Ok(r) => r,
        Err(_) => {
            // Error already printed by facet-args
            std::process::exit(1);
        }
    };

    println!("✅ All required fields are set!");
    println!();
    println!("Configuration:");
    println!("  SMTP Host: {}", result.value.config.email.host);
    println!("  SMTP Port: {}", result.value.config.email.port);
    println!("  From: {}", result.value.config.email.from);
    if let Some(username) = &result.value.config.email.username {
        println!("  Username: {}", username);
    }
    if result.value.config.email.password.is_some() {
        println!("  Password: <set but hidden>");
    }
}
