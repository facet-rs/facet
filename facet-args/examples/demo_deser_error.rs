//! Demonstration of deserialization errors.
//!
//! This example shows how facet-args handles type mismatches and other
//! deserialization errors with helpful diagnostics.
//!
//! Run with different scenarios:
//!
//! ```bash
//! # Type error: string where number expected
//! cargo run --example demo_deser_error -- --config.port not-a-number
//!
//! # Working example
//! cargo run --example demo_deser_error -- --config.port 8080
//! ```

use facet::Facet;
use facet_args as args;

/// Application with typed configuration.
#[derive(Facet)]
struct Args {
    /// Application configuration.
    #[facet(args::config, args::env_prefix = "MYAPP")]
    config: AppConfig,
}

/// Application configuration with typed fields.
#[derive(Facet)]
#[facet(derive(Default), traits(Default))]
struct AppConfig {
    /// Server port (must be a number).
    #[facet(default = 8080)]
    port: u16,

    /// Maximum connections (must be a number).
    #[facet(default = 100)]
    max_connections: u32,

    /// Server host.
    #[facet(default = "localhost".to_string())]
    host: String,
}

fn main() {
    println!("=== Deserialization Error Demo ===\n");

    let args_vec: Vec<String> = std::env::args().skip(1).collect();
    let args_str: Vec<&str> = args_vec.iter().map(|s| s.as_str()).collect();

    // This will show a helpful error if there's a type mismatch
    let result = match facet_args::from_slice_layered::<Args>(&args_str) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    println!("✅ Configuration parsed successfully!");
    println!();
    println!("Configuration:");
    println!("  Host: {}", result.value.config.host);
    println!("  Port: {}", result.value.config.port);
    println!("  Max connections: {}", result.value.config.max_connections);
}
