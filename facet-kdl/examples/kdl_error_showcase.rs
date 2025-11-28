//! Error Showcase: Demonstrating facet-kdl error diagnostics
//!
//! This example showcases the rich error reporting capabilities of facet-kdl
//! with miette's beautiful diagnostic output.
//!
//! Run with: cargo run --example kdl_error_showcase

use facet::Facet;
use facet_kdl::from_str;
use facet_showcase::{Language, ShowcaseRunner};

fn main() {
    let mut runner = ShowcaseRunner::new("facet-kdl Error Showcase")
        .language(Language::Kdl)
        .with_kdl_syntaxes(concat!(env!("CARGO_MANIFEST_DIR"), "/syntaxes"));
    runner.header();

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

    runner.footer();
}

// ============================================================================
// Type Definitions for Error Scenarios
// ============================================================================

// --- Scenario: Ambiguous enum (identical fields) ---

#[derive(Facet, Debug)]
struct AmbiguousConfig {
    #[facet(child)]
    resource: AmbiguousResource,
}

#[derive(Facet, Debug)]
struct AmbiguousResource {
    #[facet(argument)]
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
    #[facet(property)]
    value: String,
    #[facet(property)]
    priority: u32,
}

// --- Scenario: NoMatch with candidate failures ---

#[derive(Facet, Debug)]
struct NoMatchConfig {
    #[facet(child)]
    backend: NoMatchBackend,
}

#[derive(Facet, Debug)]
struct NoMatchBackend {
    #[facet(argument)]
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
    #[facet(property)]
    database_path: String,
    #[facet(property)]
    journal_mode: String,
}

#[derive(Facet, Debug)]
struct PostgresBackend {
    #[facet(property)]
    connection_string: String,
    #[facet(property)]
    pool_size: u32,
}

#[derive(Facet, Debug)]
struct RedisBackend {
    #[facet(property)]
    host: String,
    #[facet(property)]
    port: u16,
    #[facet(property)]
    password: Option<String>,
}

// --- Scenario: Unknown fields with suggestions ---

#[derive(Facet, Debug)]
struct TypoConfig {
    #[facet(child)]
    server: TypoServer,
}

#[derive(Facet, Debug)]
struct TypoServer {
    #[facet(argument)]
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
    #[facet(property)]
    hostname: String,
    #[facet(property)]
    port: u16,
    #[facet(property)]
    ssl_enabled: bool,
}

#[derive(Facet, Debug)]
struct ApiServer {
    #[facet(property)]
    endpoint: String,
    #[facet(property)]
    timeout_ms: u32,
    #[facet(property)]
    retry_count: u8,
}

// --- Scenario: Value-based disambiguation ---

#[derive(Facet, Debug)]
struct ValueConfig {
    #[facet(child)]
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
    #[facet(property)]
    count: u8,
}

#[derive(Facet, Debug)]
struct LargeValue {
    #[facet(property)]
    count: u32,
}

// --- Scenario: Multi-line config ---

#[derive(Facet, Debug)]
struct MultiLineConfig {
    #[facet(child)]
    database: MultiLineDatabase,
}

#[derive(Facet, Debug)]
struct MultiLineDatabase {
    #[facet(argument)]
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
    #[facet(property)]
    host: String,
    #[facet(property)]
    port: u16,
    #[facet(property)]
    username: String,
    #[facet(property)]
    password: String,
}

#[derive(Facet, Debug)]
struct PgConfig {
    #[facet(property)]
    host: String,
    #[facet(property)]
    port: u16,
    #[facet(property)]
    database: String,
    #[facet(property)]
    ssl_mode: String,
}

#[derive(Facet, Debug)]
struct MongoConfig {
    #[facet(property)]
    uri: String,
    #[facet(property)]
    replica_set: Option<String>,
}

// --- Scenario: Basic struct with deny_unknown_fields ---

#[derive(Facet, Debug)]
#[facet(deny_unknown_fields)]
struct SimpleConfig {
    #[facet(child)]
    server: SimpleServer,
}

#[derive(Facet, Debug)]
#[facet(deny_unknown_fields)]
struct SimpleServer {
    #[facet(property)]
    host: String,
    #[facet(property)]
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
