//! Example: CLI tool that supports `@dump-styx-schema`
//!
//! This demonstrates the recommended pattern for schema discovery.
//! Run with: `cargo run --example dump_schema -- @dump-styx-schema`

use std::path::PathBuf;

use facet::Facet;

/// Server configuration.
#[derive(Facet, Debug)]
struct Config {
    /// Hostname or IP address to bind to.
    host: String,

    /// Port number (1-65535).
    port: u16,

    /// Request timeout in seconds.
    timeout_secs: u64,

    /// TLS configuration (optional).
    tls: Option<TlsConfig>,

    /// Allowed origins for CORS.
    allowed_origins: Vec<String>,
}

/// TLS certificate configuration.
#[derive(Facet, Debug)]
struct TlsConfig {
    /// Path to certificate file.
    cert: PathBuf,
    /// Path to private key file.
    key: PathBuf,
}

fn main() {
    // Handle @dump-styx-schema before anything else
    if std::env::args().nth(1).as_deref() == Some("@dump-styx-schema") {
        print_schema();
        return;
    }

    // Normal application logic
    println!("This is myapp. Use @dump-styx-schema to dump the config schema.");
    println!();
    println!("Example config (config.styx):");
    println!();
    println!("  @schema {{source crate:myapp-config@1, cli myapp}}");
    println!();
    println!("  host localhost");
    println!("  port 8080");
    println!("  timeout_secs 30");
}

/// Print the schema to stdout.
///
/// In a real application, this would use `StyxSchema::builder()` to generate
/// the schema automatically from the Facet types. For now, we output it manually
/// to demonstrate the expected format.
fn print_schema() {
    // TODO: Replace with automatic generation once StyxSchema::builder() exists:
    //
    // let schema = StyxSchema::builder()
    //     .crate_name("myapp-config")
    //     .version(env!("CARGO_PKG_VERSION"))
    //     .bin("myapp")
    //     .root::<Config>()
    //     .build();
    // println!("{schema}");

    print!(
        r#"@meta {{
  crate myapp-config
  version {version}
  bin myapp
}}

/// Server configuration.
Config @object {{
  /// Hostname or IP address to bind to.
  host @default(localhost @string)

  /// Port number (1-65535).
  port @default(8080 @int{{ min 1, max 65535 }})

  /// Request timeout in seconds.
  timeout_secs @default(30 @int)

  /// TLS configuration (optional).
  tls @optional(TlsConfig)

  /// Allowed origins for CORS.
  allowed_origins @default(() @list(@string))
}}

/// TLS certificate configuration.
TlsConfig @object {{
  /// Path to certificate file.
  cert @string
  /// Path to private key file.
  key @string
}}
"#,
        version = env!("CARGO_PKG_VERSION")
    );
}
