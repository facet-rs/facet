//! # Syntax Highlighting Test Crate
//!
//! This crate exists solely to test that our custom highlight.js bundle
//! works correctly with rustdoc output.
//!
//! ## KDL Example
//!
//! ```kdl
//! // Server configuration
//! server "localhost" port=8080 {
//!     timeout 30
//!     ssl enabled=true cert="/path/to/cert.pem"
//!
//!     routes {
//!         route "/api" handler="api_handler"
//!         route "/static" handler="static_files" {
//!             cache max-age=3600
//!         }
//!     }
//! }
//!
//! // Database settings
//! database {
//!     host "db.example.com"
//!     port 5432
//!     username "admin"
//!     pool-size 10
//!     ssl true
//! }
//! ```
//!
//! ## JSON Example
//!
//! ```json
//! {
//!   "name": "facet",
//!   "version": "0.31.0",
//!   "features": {
//!     "reflection": true,
//!     "serialization": ["json", "yaml", "toml", "kdl"]
//!   },
//!   "dependencies": {
//!     "facet-core": "0.31.0",
//!     "facet-reflect": "0.31.0"
//!   },
//!   "count": 42,
//!   "enabled": true,
//!   "ratio": 3.14159
//! }
//! ```
//!
//! ## YAML Example
//!
//! ```yaml
//! # Application configuration
//! app:
//!   name: facet-demo
//!   version: "1.0.0"
//!   debug: false
//!
//! server:
//!   host: localhost
//!   port: 8080
//!   workers: 4
//!
//! database:
//!   url: postgres://localhost/facet
//!   pool_size: 10
//!   timeout: 30
//!
//! features:
//!   - reflection
//!   - serialization
//!   - deserialization
//! ```
//!
//! ## TOML Example
//!
//! ```toml
//! # Package configuration
//! [package]
//! name = "facet"
//! version = "0.31.0"
//! edition = "2024"
//! rust-version = "1.87"
//!
//! [features]
//! default = ["std"]
//! std = ["alloc"]
//! alloc = []
//!
//! [dependencies]
//! facet-core = { path = "../facet-core", version = "0.31.0" }
//! facet-reflect = { path = "../facet-reflect", optional = true }
//!
//! [dev-dependencies]
//! insta = "1.43.1"
//!
//! [[bin]]
//! name = "facet-cli"
//! path = "src/main.rs"
//! ```
//!
//! ## Bash Example
//!
//! ```bash
//! #!/bin/bash
//! set -euo pipefail
//!
//! # Build the project
//! cargo build --release
//!
//! # Run tests
//! cargo nextest run --all-features
//!
//! # Generate docs
//! RUSTDOCFLAGS="--html-in-header docs/meta/highlight.html" \
//!     cargo doc --no-deps --all-features
//!
//! echo "Build complete!"
//! ```
//!
//! ## XML/HTML Example
//!
//! ```html
//! <!DOCTYPE html>
//! <html lang="en">
//! <head>
//!     <meta charset="UTF-8">
//!     <title>Facet Demo</title>
//!     <link rel="stylesheet" href="styles.css">
//! </head>
//! <body>
//!     <div id="app">
//!         <h1>Hello, Facet!</h1>
//!         <p class="description">Runtime reflection for Rust</p>
//!     </div>
//!     <script src="main.js"></script>
//! </body>
//! </html>
//! ```
//!
//! ## Plain Text Example
//!
//! ```text
//! This is plain text with no syntax highlighting.
//! It should just be displayed as-is without any colors.
//!
//!     Indented text
//!     More indented text
//!
//! Special characters: <>&"'
//! ```

/// A struct to make this a valid crate
pub struct Test;
