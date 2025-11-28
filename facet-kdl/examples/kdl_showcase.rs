//! Showcase of facet-kdl serialization and deserialization
//!
//! This example demonstrates various serialization scenarios with
//! syntax-highlighted KDL output and Rust type definitions.
//!
//! Run with: cargo run --example kdl_showcase

use facet::Facet;
use facet_kdl::{from_str, to_string};
use facet_showcase::{Language, ShowcaseRunner};

fn main() {
    let mut runner = ShowcaseRunner::new("facet-kdl Serialization Showcase")
        .language(Language::Kdl)
        .with_kdl_syntaxes(concat!(env!("CARGO_MANIFEST_DIR"), "/syntaxes"));
    runner.header();

    // =========================================================================
    // Serialization Examples
    // =========================================================================

    scenario_basic_properties(&mut runner);
    scenario_node_with_argument(&mut runner);
    scenario_nested_nodes(&mut runner);
    scenario_vec_children(&mut runner);
    scenario_complex_config(&mut runner);

    // =========================================================================
    // Roundtrip Demonstration
    // =========================================================================

    scenario_roundtrip(&mut runner);

    runner.footer();
}

// ============================================================================
// Type Definitions
// ============================================================================

// --- Basic Node with Properties ---
#[derive(Facet, Debug)]
struct Person {
    #[facet(property)]
    name: String,
    #[facet(property)]
    age: u32,
    #[facet(property)]
    email: Option<String>,
}

#[derive(Facet, Debug)]
struct PersonDoc {
    #[facet(child)]
    person: Person,
}

// --- Node with Argument ---
#[derive(Facet, Debug)]
struct Server {
    #[facet(argument)]
    name: String,
    #[facet(property)]
    host: String,
    #[facet(property)]
    port: u16,
}

#[derive(Facet, Debug)]
struct ServerDoc {
    #[facet(child)]
    server: Server,
}

// --- Nested Nodes ---
#[derive(Facet, Debug)]
struct Address {
    #[facet(property)]
    street: String,
    #[facet(property)]
    city: String,
}

#[derive(Facet, Debug)]
struct Company {
    #[facet(property)]
    name: String,
    #[facet(child)]
    address: Address,
}

#[derive(Facet, Debug)]
struct CompanyDoc {
    #[facet(child)]
    company: Company,
}

// --- Vec as Repeated Children ---
#[derive(Facet, Debug)]
struct Member {
    #[facet(argument)]
    name: String,
    #[facet(property)]
    role: String,
}

#[derive(Facet, Debug)]
struct TeamDoc {
    #[facet(children)]
    member: Vec<Member>,
}

// --- Simple Config for Roundtrip ---
#[derive(Facet, Debug)]
struct Config {
    #[facet(property)]
    debug: bool,
    #[facet(property)]
    max_connections: u32,
    #[facet(property)]
    timeout_ms: u32,
}

#[derive(Facet, Debug)]
struct ConfigDoc {
    #[facet(child)]
    config: Config,
}

// --- Complex Nested Config ---
#[derive(Facet, Debug)]
struct TlsConfig {
    #[facet(property)]
    cert_path: String,
    #[facet(property)]
    key_path: String,
}

#[derive(Facet, Debug)]
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

#[derive(Facet, Debug)]
struct DatabaseConfig {
    #[facet(argument)]
    name: String,
    #[facet(property)]
    url: String,
    #[facet(property)]
    pool_size: u32,
}

#[derive(Facet, Debug)]
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

// ============================================================================
// Scenarios
// ============================================================================

fn scenario_basic_properties(runner: &mut ShowcaseRunner) {
    let value = PersonDoc {
        person: Person {
            name: "Alice".to_string(),
            age: 30,
            email: Some("alice@example.com".to_string()),
        },
    };

    let kdl = to_string(&value).unwrap();
    let result: Result<PersonDoc, _> = from_str(&kdl);

    runner
        .scenario("Basic Node with Properties")
        .description("Simple struct with `#[facet(property)]` fields becomes KDL properties.")
        .input(Language::Kdl, &kdl)
        .target_type::<PersonDoc>()
        .result(&result)
        .finish();
}

fn scenario_node_with_argument(runner: &mut ShowcaseRunner) {
    let value = ServerDoc {
        server: Server {
            name: "web-01".to_string(),
            host: "localhost".to_string(),
            port: 8080,
        },
    };

    let kdl = to_string(&value).unwrap();
    let result: Result<ServerDoc, _> = from_str(&kdl);

    runner
        .scenario("Node with Argument")
        .description(
            "`#[facet(argument)]` field becomes a positional argument after the node name.\n\
             Result: `server \"web-01\" host=\"localhost\" port=8080`",
        )
        .input(Language::Kdl, &kdl)
        .target_type::<ServerDoc>()
        .result(&result)
        .finish();
}

fn scenario_nested_nodes(runner: &mut ShowcaseRunner) {
    let value = CompanyDoc {
        company: Company {
            name: "Acme Corp".to_string(),
            address: Address {
                street: "123 Main St".to_string(),
                city: "Springfield".to_string(),
            },
        },
    };

    let kdl = to_string(&value).unwrap();
    let result: Result<CompanyDoc, _> = from_str(&kdl);

    runner
        .scenario("Nested Nodes (Children)")
        .description(
            "`#[facet(child)]` fields become nested child nodes in braces.\n\
             The address struct becomes a child node of company.",
        )
        .input(Language::Kdl, &kdl)
        .target_type::<CompanyDoc>()
        .result(&result)
        .finish();
}

fn scenario_vec_children(runner: &mut ShowcaseRunner) {
    let value = TeamDoc {
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
    };

    let kdl = to_string(&value).unwrap();
    let result: Result<TeamDoc, _> = from_str(&kdl);

    runner
        .scenario("Vec as Repeated Children")
        .description(
            "`#[facet(children)]` on a `Vec` field creates repeated child nodes.\n\
             Each `Member` becomes a separate `member` node.",
        )
        .input(Language::Kdl, &kdl)
        .target_type::<TeamDoc>()
        .result(&result)
        .finish();
}

fn scenario_complex_config(runner: &mut ShowcaseRunner) {
    let value = AppConfig {
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
    };

    let kdl = to_string(&value).unwrap();
    let result: Result<AppConfig, _> = from_str(&kdl);

    runner
        .scenario("Complex Nested Config")
        .description(
            "A realistic application config showing:\n\
             - Top-level properties (`debug`, `features`)\n\
             - Child nodes with arguments (`server`, `database`)\n\
             - Nested children (`tls` inside `server`)\n\
             - Optional children (`tls` is `Option<TlsConfig>`)",
        )
        .input(Language::Kdl, &kdl)
        .target_type::<AppConfig>()
        .result(&result)
        .finish();
}

fn scenario_roundtrip(runner: &mut ShowcaseRunner) {
    let original = ConfigDoc {
        config: Config {
            debug: true,
            max_connections: 100,
            timeout_ms: 5000,
        },
    };

    let kdl = to_string(&original).unwrap();
    let roundtrip: Result<ConfigDoc, _> = from_str(&kdl);

    runner
        .scenario("Roundtrip: Rust → KDL → Rust")
        .description(
            "Demonstrates serialization followed by deserialization.\n\
             The value survives the roundtrip intact.",
        )
        .input(Language::Kdl, &kdl)
        .target_type::<ConfigDoc>()
        .result(&roundtrip)
        .finish();
}
