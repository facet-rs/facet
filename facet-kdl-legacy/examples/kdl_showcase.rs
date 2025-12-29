//! Showcase of facet-kdl serialization and deserialization
//!
//! This example demonstrates various serialization scenarios with
//! syntax-highlighted KDL output and Rust type definitions.
//!
//! Run with: cargo run --example kdl_showcase

use facet::Facet;
use facet_kdl_legacy as kdl;
use facet_kdl_legacy::{from_str, to_string};
use facet_showcase::{Language, ShowcaseRunner};

fn main() {
    let mut runner = ShowcaseRunner::new("KDL")
        .language(Language::Kdl)
        .with_kdl_syntaxes(concat!(env!("CARGO_MANIFEST_DIR"), "/syntaxes"));
    runner.header();
    runner.intro("[`facet-kdl`](https://docs.rs/facet-kdl) provides serialization and deserialization for [KDL](https://kdl.dev), a document language with a focus on human readability. Use attributes like `kdl::property`, `kdl::argument`, and `kdl::child` to control how your types map to KDL's node-based structure.");

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

    // =========================================================================
    // Solver Errors (flattened enums)
    // =========================================================================

    scenario_ambiguous_enum(&mut runner);
    scenario_no_match_with_failures(&mut runner);
    scenario_typo_suggestions(&mut runner);
    scenario_value_overflow(&mut runner);
    scenario_multiline(&mut runner);

    // =========================================================================
    // Basic Errors
    // =========================================================================

    scenario_unknown_field(&mut runner);
    scenario_missing_field(&mut runner);

    // =========================================================================
    // Syntax Errors
    // =========================================================================

    scenario_syntax_error_unquoted_bool(&mut runner);
    scenario_syntax_error_unclosed_brace(&mut runner);

    runner.footer();
}

// ============================================================================
// Type Definitions
// ============================================================================

// --- Basic Node with Properties ---
#[derive(Facet, Debug)]
struct Person {
    #[facet(kdl::property)]
    name: String,
    #[facet(kdl::property)]
    age: u32,
    #[facet(kdl::property)]
    email: Option<String>,
}

#[derive(Facet, Debug)]
struct PersonDoc {
    #[facet(kdl::child)]
    person: Person,
}

// --- Node with Argument ---
#[derive(Facet, Debug)]
struct Server {
    #[facet(kdl::argument)]
    name: String,
    #[facet(kdl::property)]
    host: String,
    #[facet(kdl::property)]
    port: u16,
}

#[derive(Facet, Debug)]
struct ServerDoc {
    #[facet(kdl::child)]
    server: Server,
}

// --- Nested Nodes ---
#[derive(Facet, Debug)]
struct Address {
    #[facet(kdl::property)]
    street: String,
    #[facet(kdl::property)]
    city: String,
}

#[derive(Facet, Debug)]
struct Company {
    #[facet(kdl::property)]
    name: String,
    #[facet(kdl::child)]
    address: Address,
}

#[derive(Facet, Debug)]
struct CompanyDoc {
    #[facet(kdl::child)]
    company: Company,
}

// --- Vec as Repeated Children ---
#[derive(Facet, Debug)]
struct Member {
    #[facet(kdl::argument)]
    name: String,
    #[facet(kdl::property)]
    role: String,
}

#[derive(Facet, Debug)]
struct TeamDoc {
    #[facet(kdl::children)]
    member: Vec<Member>,
}

// --- Simple Config for Roundtrip ---
#[derive(Facet, Debug)]
struct Config {
    #[facet(kdl::property)]
    debug: bool,
    #[facet(kdl::property)]
    max_connections: u32,
    #[facet(kdl::property)]
    timeout_ms: u32,
}

#[derive(Facet, Debug)]
struct ConfigDoc {
    #[facet(kdl::child)]
    config: Config,
}

// --- Complex Nested Config ---
#[derive(Facet, Debug)]
struct TlsConfig {
    #[facet(kdl::property)]
    cert_path: String,
    #[facet(kdl::property)]
    key_path: String,
}

#[derive(Facet, Debug)]
struct ServerConfig {
    #[facet(kdl::argument)]
    name: String,
    #[facet(kdl::property)]
    host: String,
    #[facet(kdl::property)]
    port: u16,
    #[facet(kdl::child)]
    tls: Option<TlsConfig>,
}

#[derive(Facet, Debug)]
struct DatabaseConfig {
    #[facet(kdl::argument)]
    name: String,
    #[facet(kdl::property)]
    url: String,
    #[facet(kdl::property)]
    pool_size: u32,
}

#[derive(Facet, Debug)]
struct AppConfig {
    #[facet(kdl::property)]
    debug: bool,
    #[facet(kdl::child)]
    server: ServerConfig,
    #[facet(kdl::child)]
    database: DatabaseConfig,
    #[facet(kdl::property)]
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
        .description("Simple struct with `#[facet(kdl::property)]` fields becomes KDL properties.")
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
            "`#[facet(kdl::argument)]` field becomes a positional argument after the node name.\n\
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
            "`#[facet(kdl::child)]` fields become nested child nodes in braces.\n\
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
            "`#[facet(kdl::children)]` on a `Vec` field creates repeated child nodes.\n\
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

// ============================================================================
// Type Definitions for Error Scenarios
// ============================================================================

// --- Scenario: Ambiguous enum (identical fields) ---

#[derive(Facet, Debug)]
struct AmbiguousConfig {
    #[facet(kdl::child)]
    resource: AmbiguousResource,
}

#[derive(Facet, Debug)]
struct AmbiguousResource {
    #[facet(kdl::argument)]
    name: String,
    #[facet(flatten)]
    kind: AmbiguousKind,
}

#[derive(Facet, Debug)]
#[repr(u8)]
#[allow(dead_code)]
enum AmbiguousKind {
    // Both variants have identical fields - truly ambiguous!
    TypeA(CommonFields),
    TypeB(CommonFields),
}

#[derive(Facet, Debug)]
struct CommonFields {
    #[facet(kdl::property)]
    value: String,
    #[facet(kdl::property)]
    priority: u32,
}

// --- Scenario: NoMatch with candidate failures ---

#[derive(Facet, Debug)]
struct NoMatchConfig {
    #[facet(kdl::child)]
    backend: NoMatchBackend,
}

#[derive(Facet, Debug)]
struct NoMatchBackend {
    #[facet(kdl::argument)]
    name: String,
    #[facet(flatten)]
    kind: NoMatchKind,
}

#[derive(Facet, Debug)]
#[repr(u8)]
#[allow(dead_code)]
enum NoMatchKind {
    Sqlite(SqliteBackend),
    Postgres(PostgresBackend),
    Redis(RedisBackend),
}

#[derive(Facet, Debug)]
struct SqliteBackend {
    #[facet(kdl::property)]
    database_path: String,
    #[facet(kdl::property)]
    journal_mode: String,
}

#[derive(Facet, Debug)]
struct PostgresBackend {
    #[facet(kdl::property)]
    connection_string: String,
    #[facet(kdl::property)]
    pool_size: u32,
}

#[derive(Facet, Debug)]
struct RedisBackend {
    #[facet(kdl::property)]
    host: String,
    #[facet(kdl::property)]
    port: u16,
    #[facet(kdl::property)]
    password: Option<String>,
}

// --- Scenario: Unknown fields with suggestions ---

#[derive(Facet, Debug)]
struct TypoConfig {
    #[facet(kdl::child)]
    server: TypoServer,
}

#[derive(Facet, Debug)]
struct TypoServer {
    #[facet(kdl::argument)]
    name: String,
    #[facet(flatten)]
    kind: TypoKind,
}

#[derive(Facet, Debug)]
#[repr(u8)]
#[allow(dead_code)]
enum TypoKind {
    Web(WebServer),
    Api(ApiServer),
}

#[derive(Facet, Debug)]
struct WebServer {
    #[facet(kdl::property)]
    hostname: String,
    #[facet(kdl::property)]
    port: u16,
    #[facet(kdl::property)]
    ssl_enabled: bool,
}

#[derive(Facet, Debug)]
struct ApiServer {
    #[facet(kdl::property)]
    endpoint: String,
    #[facet(kdl::property)]
    timeout_ms: u32,
    #[facet(kdl::property)]
    retry_count: u8,
}

// --- Scenario: Value-based disambiguation ---

#[derive(Facet, Debug)]
struct ValueConfig {
    #[facet(kdl::child)]
    data: ValueData,
}

#[derive(Facet, Debug)]
struct ValueData {
    #[facet(flatten)]
    payload: ValuePayload,
}

#[derive(Facet, Debug)]
#[repr(u8)]
#[allow(dead_code)]
enum ValuePayload {
    Small(SmallValue),
    Large(LargeValue),
}

#[derive(Facet, Debug)]
struct SmallValue {
    #[facet(kdl::property)]
    count: u8,
}

#[derive(Facet, Debug)]
struct LargeValue {
    #[facet(kdl::property)]
    count: u32,
}

// --- Scenario: Multi-line config ---

#[derive(Facet, Debug)]
struct MultiLineConfig {
    #[facet(kdl::child)]
    database: MultiLineDatabase,
}

#[derive(Facet, Debug)]
struct MultiLineDatabase {
    #[facet(kdl::argument)]
    name: String,
    #[facet(flatten)]
    kind: MultiLineDbKind,
}

#[derive(Facet, Debug)]
#[repr(u8)]
#[allow(dead_code)]
enum MultiLineDbKind {
    MySql(MySqlConfig),
    Postgres(PgConfig),
    Mongo(MongoConfig),
}

#[derive(Facet, Debug)]
struct MySqlConfig {
    #[facet(kdl::property)]
    host: String,
    #[facet(kdl::property)]
    port: u16,
    #[facet(kdl::property)]
    username: String,
    #[facet(kdl::property)]
    password: String,
}

#[derive(Facet, Debug)]
struct PgConfig {
    #[facet(kdl::property)]
    host: String,
    #[facet(kdl::property)]
    port: u16,
    #[facet(kdl::property)]
    database: String,
    #[facet(kdl::property)]
    ssl_mode: String,
}

#[derive(Facet, Debug)]
struct MongoConfig {
    #[facet(kdl::property)]
    uri: String,
    #[facet(kdl::property)]
    replica_set: Option<String>,
}

// --- Scenario: Basic struct with deny_unknown_fields ---

#[derive(Facet, Debug)]
#[facet(deny_unknown_fields)]
struct SimpleConfig {
    #[facet(kdl::child)]
    server: SimpleServer,
}

#[derive(Facet, Debug)]
#[facet(deny_unknown_fields)]
struct SimpleServer {
    #[facet(kdl::property)]
    host: String,
    #[facet(kdl::property)]
    port: u16,
}

// ============================================================================
// Error Scenarios
// ============================================================================

fn scenario_ambiguous_enum(runner: &mut ShowcaseRunner) {
    let kdl = r#"resource "test" value="hello" priority=10"#;
    let result: Result<AmbiguousConfig, _> = from_str(kdl);

    runner
        .scenario("Ambiguous Flattened Enum")
        .description(
            "Both TypeA and TypeB variants have identical fields (value, priority).\n\
             The solver cannot determine which variant to use.",
        )
        .input(Language::Kdl, kdl)
        .target_type::<AmbiguousConfig>()
        .result(&result)
        .finish();
}

fn scenario_no_match_with_failures(runner: &mut ShowcaseRunner) {
    // Misspelled field names that trigger NoMatch
    let kdl = r#"backend "cache" hst="localhost" conn_str="pg""#;
    let result: Result<NoMatchConfig, _> = from_str(kdl);

    runner
        .scenario("NoMatch with Per-Candidate Failures")
        .description(
            "Provide field names that don't exactly match any variant.\n\
             The solver shows WHY each candidate failed with 'did you mean?' suggestions.",
        )
        .input(Language::Kdl, kdl)
        .target_type::<NoMatchConfig>()
        .result(&result)
        .finish();
}

fn scenario_typo_suggestions(runner: &mut ShowcaseRunner) {
    // Typos: 'hostnam' instead of 'hostname', 'prot' instead of 'port'
    let kdl = r#"server "web" hostnam="localhost" prot=8080"#;
    let result: Result<TypoConfig, _> = from_str(kdl);

    runner
        .scenario("Unknown Fields with 'Did You Mean?' Suggestions")
        .description(
            "Misspell field names and see the solver suggest corrections!\n\
             Uses Jaro-Winkler similarity to find close matches.",
        )
        .input(Language::Kdl, kdl)
        .target_type::<TypoConfig>()
        .result(&result)
        .finish();
}

fn scenario_value_overflow(runner: &mut ShowcaseRunner) {
    // Value too large - doesn't fit u8 (max 255) or u32 (max ~4B)
    let kdl = r#"data count=5000000000"#;
    let result: Result<ValueConfig, _> = from_str(kdl);

    runner
        .scenario("Value Overflow Detection")
        .description(
            "When a value doesn't fit ANY candidate type, the solver reports it.\n\
             count=5000000000 exceeds both u8 (max 255) and u32 (max ~4 billion).",
        )
        .input(Language::Kdl, kdl)
        .target_type::<ValueConfig>()
        .result(&result)
        .finish();
}

fn scenario_multiline(runner: &mut ShowcaseRunner) {
    // Multi-line KDL with typos
    let kdl = r#"database "production" \
    hots="db.example.com" \
    prot=3306 \
    usernme="admin" \
    pasword="secret123"
"#;
    let result: Result<MultiLineConfig, _> = from_str(kdl);

    runner
        .scenario("Multi-Line Config with Typos")
        .description(
            "A more realistic multi-line configuration file with several typos.\n\
             Shows how the solver sorts candidates by closeness to the input.",
        )
        .input(Language::Kdl, kdl)
        .target_type::<MultiLineConfig>()
        .result(&result)
        .finish();
}

fn scenario_unknown_field(runner: &mut ShowcaseRunner) {
    let kdl = r#"server host="localhost" prot=8080"#;
    let result: Result<SimpleConfig, _> = from_str(kdl);

    runner
        .scenario("Unknown Field")
        .description(
            "KDL contains a property that doesn't exist in the target struct.\n\
             With #[facet(deny_unknown_fields)], this is an error.",
        )
        .input(Language::Kdl, kdl)
        .target_type::<SimpleConfig>()
        .result(&result)
        .finish();
}

fn scenario_missing_field(runner: &mut ShowcaseRunner) {
    let kdl = r#"server host="localhost""#;
    let result: Result<SimpleConfig, _> = from_str(kdl);

    runner
        .scenario("Missing Required Field")
        .description("KDL is missing a required field that has no default.")
        .input(Language::Kdl, kdl)
        .target_type::<SimpleConfig>()
        .result(&result)
        .finish();
}

fn scenario_syntax_error_unquoted_bool(runner: &mut ShowcaseRunner) {
    // In KDL 2.0, booleans must use #true/#false, not bare true/false
    let kdl = r#"server host="localhost" enabled=true"#;
    let result: Result<SimpleConfig, _> = from_str(kdl);

    runner
        .scenario("Syntax Error: Unquoted Boolean")
        .description(
            "KDL 2.0 requires booleans to be written as #true/#false.\n\
             Bare `true` or `false` is a syntax error with a helpful message.",
        )
        .input(Language::Kdl, kdl)
        .target_type::<SimpleConfig>()
        .result(&result)
        .finish();
}

fn scenario_syntax_error_unclosed_brace(runner: &mut ShowcaseRunner) {
    let kdl = r#"server host="localhost" port=8080 {
    tls cert="/path/to/cert"
"#;
    let result: Result<SimpleConfig, _> = from_str(kdl);

    runner
        .scenario("Syntax Error: Unclosed Brace")
        .description(
            "Missing closing brace in nested node structure.\n\
             The parser provides line/column information for the error.",
        )
        .input(Language::Kdl, kdl)
        .target_type::<SimpleConfig>()
        .result(&result)
        .finish();
}
