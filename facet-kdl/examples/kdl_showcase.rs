//! Showcase of facet-kdl serialization and deserialization
//!
//! This example demonstrates various serialization scenarios with
//! syntax-highlighted KDL output and Rust type definitions via facet-pretty.
//!
//! KDL is a document language with a unique node-based structure:
//! - Nodes have names, arguments, properties, and children
//! - Properties are key=value pairs
//! - Arguments are positional values
//! - Children are nested nodes in braces
//!
//! Run with: cargo run --example showcase

use facet::Facet;
use syntect::easy::HighlightLines;
use syntect::highlighting::Style;
use syntect::parsing::{SyntaxSet, SyntaxSetBuilder};
use syntect::util::{LinesWithEndings, as_24_bit_terminal_escaped};

/// Highlighter that can handle both Rust (from defaults) and KDL (custom syntax)
struct Highlighter {
    rust_ps: SyntaxSet,
    kdl_ps: SyntaxSet,
    theme: syntect::highlighting::Theme,
}

impl Highlighter {
    fn new() -> Self {
        // Load default syntaxes (for Rust)
        let rust_ps = SyntaxSet::load_defaults_newlines();

        // Build syntax set with KDL support
        let mut builder = SyntaxSetBuilder::new();
        builder.add_plain_text_syntax();
        builder
            .add_from_folder(concat!(env!("CARGO_MANIFEST_DIR"), "/syntaxes"), true)
            .expect("Failed to load KDL syntaxes");
        let kdl_ps = builder.build();

        // Load Monokai theme
        let theme_path = concat!(env!("CARGO_MANIFEST_DIR"), "/themes/Monokai.tmTheme");
        let theme = syntect::highlighting::ThemeSet::get_theme(theme_path)
            .expect("Failed to load Monokai theme");

        Self {
            rust_ps,
            kdl_ps,
            theme,
        }
    }

    fn highlight_rust(&self, code: &str) {
        let syntax = self.rust_ps.find_syntax_by_extension("rs").unwrap();
        let mut h = HighlightLines::new(syntax, &self.theme);
        for line in LinesWithEndings::from(code) {
            let ranges: Vec<(Style, &str)> = h.highlight_line(line, &self.rust_ps).unwrap();
            let escaped = as_24_bit_terminal_escaped(&ranges[..], false);
            print!("    {escaped}");
        }
        println!("\x1b[0m");
    }

    fn highlight_kdl(&self, code: &str) {
        let syntax = self.kdl_ps.find_syntax_by_name("KDL1").unwrap();
        let mut h = HighlightLines::new(syntax, &self.theme);
        for line in LinesWithEndings::from(code) {
            let ranges: Vec<(Style, &str)> = h.highlight_line(line, &self.kdl_ps).unwrap();
            let escaped = as_24_bit_terminal_escaped(&ranges[..], false);
            print!("    {escaped}");
        }
        println!("\x1b[0m");
    }
}

fn main() {
    let hl = Highlighter::new();

    println!("\n{}", "═".repeat(70));
    println!("  facet-kdl Serialization Showcase");
    println!("{}\n", "═".repeat(70));

    // =========================================================================
    // Basic Node with Properties
    // =========================================================================
    showcase(
        &hl,
        "Basic Node with Properties",
        &PersonDoc {
            person: Person {
                name: "Alice".to_string(),
                age: 30,
                email: Some("alice@example.com".to_string()),
            },
        },
    );

    // =========================================================================
    // Node with Argument
    // =========================================================================
    showcase(
        &hl,
        "Node with Argument (#[facet(argument)])",
        &ServerDoc {
            server: Server {
                name: "web-01".to_string(),
                host: "localhost".to_string(),
                port: 8080,
            },
        },
    );

    // =========================================================================
    // Nested Nodes (Children)
    // =========================================================================
    showcase(
        &hl,
        "Nested Nodes (children)",
        &CompanyDoc {
            company: Company {
                name: "Acme Corp".to_string(),
                address: Address {
                    street: "123 Main St".to_string(),
                    city: "Springfield".to_string(),
                },
            },
        },
    );

    // =========================================================================
    // Vec as Repeated Children
    // =========================================================================
    showcase(
        &hl,
        "Vec as Repeated Children",
        &TeamDoc {
            member: vec![
                Member {
                    name: "Bob".to_string(),
                    role: "Engineer".to_string(),
                },
                Member {
                    name: "Carol".to_string(),
                    role: "Designer".to_string(),
                },
                Member {
                    name: "Dave".to_string(),
                    role: "Manager".to_string(),
                },
            ],
        },
    );

    // =========================================================================
    // Complex Config Example
    // =========================================================================
    showcase(
        &hl,
        "Complex Nested Config",
        &AppConfig {
            debug: true,
            server: ServerConfig {
                name: "api-gateway".to_string(),
                host: "0.0.0.0".to_string(),
                port: 443,
                tls: Some(TlsConfig {
                    cert_path: "/etc/ssl/cert.pem".to_string(),
                    key_path: "/etc/ssl/key.pem".to_string(),
                }),
            },
            database: DatabaseConfig {
                name: "primary".to_string(),
                url: "postgres://localhost/mydb".to_string(),
                pool_size: 10,
            },
            features: vec![
                "auth".to_string(),
                "logging".to_string(),
                "metrics".to_string(),
            ],
        },
    );

    // =========================================================================
    // Roundtrip Demonstration
    // =========================================================================
    println!("{}", "─".repeat(70));
    println!("  Roundtrip Demonstration");
    println!("{}\n", "─".repeat(70));

    let original = ConfigDoc {
        config: Config {
            debug: true,
            max_connections: 100,
            timeout_ms: 5000,
        },
    };

    println!("Original Rust value:");
    let rust_def = facet_pretty::format_shape(ConfigDoc::SHAPE);
    hl.highlight_rust(&rust_def);

    let peek = facet_reflect::Peek::new(&original);
    let pretty_value = facet_pretty::PrettyPrinter::new()
        .with_colors(false)
        .format_peek(peek);
    hl.highlight_rust(&pretty_value);

    println!("\nSerialized to KDL:");
    let kdl = facet_kdl::to_string(&original).unwrap();
    hl.highlight_kdl(&kdl);

    println!("\nDeserialized back to Rust:");
    let roundtrip: ConfigDoc = facet_kdl::from_str(&kdl).unwrap();
    let peek = facet_reflect::Peek::new(&roundtrip);
    let pretty_value = facet_pretty::PrettyPrinter::new()
        .with_colors(false)
        .format_peek(peek);
    hl.highlight_rust(&pretty_value);

    println!("\n{}", "═".repeat(70));
}

fn showcase<T: facet::Facet<'static>>(hl: &Highlighter, title: &str, value: &T) {
    println!("{}", "─".repeat(70));
    println!("  {title}");
    println!("{}\n", "─".repeat(70));

    println!("Rust definition:");
    let rust_def = facet_pretty::format_shape(T::SHAPE);
    hl.highlight_rust(&rust_def);

    println!("\nValue (via facet-pretty):");
    let peek = facet_reflect::Peek::new(value);
    let pretty_value = facet_pretty::PrettyPrinter::new()
        .with_colors(false)
        .format_peek(peek);
    hl.highlight_rust(&pretty_value);

    println!("\nKDL output:");
    let kdl = facet_kdl::to_string(value).unwrap();
    hl.highlight_kdl(&kdl);
    println!();
}

// ============================================================================
// Type definitions for the showcase
// ============================================================================
// KDL documents need a "wrapper" struct with #[facet(child)] fields to
// represent the top-level nodes.

// --- Basic Node with Properties ---
#[derive(Facet)]
struct Person {
    #[facet(property)]
    name: String,
    #[facet(property)]
    age: u32,
    #[facet(property)]
    email: Option<String>,
}

#[derive(Facet)]
struct PersonDoc {
    #[facet(child)]
    person: Person,
}

// --- Node with Argument ---
#[derive(Facet)]
struct Server {
    #[facet(argument)]
    name: String,
    #[facet(property)]
    host: String,
    #[facet(property)]
    port: u16,
}

#[derive(Facet)]
struct ServerDoc {
    #[facet(child)]
    server: Server,
}

// --- Nested Nodes ---
#[derive(Facet)]
struct Address {
    #[facet(property)]
    street: String,
    #[facet(property)]
    city: String,
}

#[derive(Facet)]
struct Company {
    #[facet(property)]
    name: String,
    #[facet(child)]
    address: Address,
}

#[derive(Facet)]
struct CompanyDoc {
    #[facet(child)]
    company: Company,
}

// --- Vec as Repeated Children ---
#[derive(Facet)]
struct Member {
    #[facet(argument)]
    name: String,
    #[facet(property)]
    role: String,
}

#[derive(Facet)]
struct TeamDoc {
    #[facet(children)]
    member: Vec<Member>,
}

// --- Simple Config for Roundtrip ---
#[derive(Facet)]
struct Config {
    #[facet(property)]
    debug: bool,
    #[facet(property)]
    max_connections: u32,
    #[facet(property)]
    timeout_ms: u32,
}

#[derive(Facet)]
struct ConfigDoc {
    #[facet(child)]
    config: Config,
}

// --- Complex Nested Config ---
#[derive(Facet)]
struct TlsConfig {
    #[facet(property)]
    cert_path: String,
    #[facet(property)]
    key_path: String,
}

#[derive(Facet)]
struct ServerConfig {
    #[facet(argument)]
    name: String,
    #[facet(property)]
    host: String,
    #[facet(property)]
    port: u16,
    #[facet(child)]
    tls: Option<TlsConfig>,
}

#[derive(Facet)]
struct DatabaseConfig {
    #[facet(argument)]
    name: String,
    #[facet(property)]
    url: String,
    #[facet(property)]
    pool_size: u32,
}

#[derive(Facet)]
struct AppConfig {
    #[facet(property)]
    debug: bool,
    #[facet(child)]
    server: ServerConfig,
    #[facet(child)]
    database: DatabaseConfig,
    #[facet(property)]
    features: Vec<String>,
}
