//! Error Showcase: Demonstrating REAL facet-kdl + facet-solver error diagnostics
//!
//! This example showcases the rich error reporting capabilities of facet-kdl.
//! ALL help text is generated from REAL error data - no hardcoded messages!
//!
//! Run with: cargo run --example error_showcase
//!
//! Scenarios:
//! 1. Ambiguous flattened enums (identical fields across variants)
//! 2. NoMatch with per-candidate failure reasons
//! 3. Unknown fields with "did you mean?" suggestions
//! 4. Value-based disambiguation success

use boxen::{BorderStyle, TextAlignment};
use facet::Facet;
use facet_kdl::{KdlError, KdlErrorKind};
use facet_pretty::FacetPretty;
use facet_solver::SolverError;
use miette::{
    Diagnostic, GraphicalReportHandler, GraphicalTheme, NamedSource, SourceSpan,
    highlighters::SyntectHighlighter,
};
use owo_colors::OwoColorize;
use std::fmt;

// Syntect imports for KDL syntax highlighting
use syntect::parsing::SyntaxSetBuilder;

// ============================================================================
// Type Definitions for Error Scenarios
// ============================================================================

// --- Scenario 1: Ambiguous enum (identical fields) ---

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

// --- Scenario 2: NoMatch with candidate failures ---

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
    // Intentionally ordered "wrong" to test sorting by closeness
    // Sqlite has NO fields matching "hst" or "conn_str"
    Sqlite(SqliteBackend),
    // Postgres has conn_str → connection_string suggestion
    Postgres(PostgresBackend),
    // Redis has hst → host suggestion (should be sorted first if we sort by suggestions!)
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

// --- Scenario 3: Unknown fields with suggestions ---

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

// --- Scenario 4: Value-based disambiguation ---

#[derive(Facet, Debug)]
struct ValueConfig {
    #[facet(child)]
    data: ValueData,
}

#[derive(Facet, Debug)]
struct ValueData {
    // No argument field - just the flattened payload
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

// --- Scenario 5: Multi-line config with nested errors ---

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

// ============================================================================
// Custom Diagnostic Error Type
// ============================================================================

/// A wrapper that adds source context to facet-kdl errors
#[derive(Debug)]
struct KdlDiagnostic {
    source: NamedSource<String>,
    message: String,
    labels: Vec<(SourceSpan, String)>,
    help: Option<String>,
}

impl KdlDiagnostic {
    fn new(filename: &str, source: &str, message: String) -> Self {
        Self {
            source: NamedSource::new(filename, source.to_string()),
            message,
            labels: Vec::new(),
            help: None,
        }
    }

    fn with_label(mut self, span: impl Into<SourceSpan>, label: impl Into<String>) -> Self {
        self.labels.push((span.into(), label.into()));
        self
    }

    fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }
}

impl fmt::Display for KdlDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for KdlDiagnostic {}

impl Diagnostic for KdlDiagnostic {
    fn code<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        Some(Box::new("facet_kdl::error"))
    }

    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        Some(&self.source)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = miette::LabeledSpan> + '_>> {
        if self.labels.is_empty() {
            None
        } else {
            Some(Box::new(self.labels.iter().map(|(span, label)| {
                miette::LabeledSpan::at(*span, label.clone())
            })))
        }
    }

    fn help<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        self.help
            .as_ref()
            .map(|h| Box::new(h) as Box<dyn fmt::Display>)
    }
}

/// Find the byte offset of a property name in KDL source
/// Returns (start, end) byte offsets
fn find_property_span(source: &str, property_name: &str) -> Option<(usize, usize)> {
    // Look for "property_name=" pattern
    let pattern = format!("{property_name}=");
    if let Some(start) = source.find(&pattern) {
        return Some((start, start + property_name.len()));
    }
    // Also try without = (in case of different syntax)
    if let Some(start) = source.find(property_name) {
        return Some((start, start + property_name.len()));
    }
    None
}

// ============================================================================
// Helper Functions
// ============================================================================

fn build_kdl_highlighter() -> SyntectHighlighter {
    let mut builder = SyntaxSetBuilder::new();
    builder.add_plain_text_syntax();
    builder
        .add_from_folder(concat!(env!("CARGO_MANIFEST_DIR"), "/syntaxes"), true)
        .expect("Failed to load KDL syntaxes");

    let syntax_set = builder.build();

    // Load Monokai theme
    let theme_path = concat!(env!("CARGO_MANIFEST_DIR"), "/themes/Monokai.tmTheme");
    let theme = syntect::highlighting::ThemeSet::get_theme(theme_path)
        .expect("Failed to load Monokai theme");

    SyntectHighlighter::new(syntax_set, theme, false)
}

fn render_error(diagnostic: &dyn Diagnostic) -> String {
    let mut output = String::new();
    let handler = GraphicalReportHandler::new_themed(GraphicalTheme::unicode())
        .with_syntax_highlighting(build_kdl_highlighter());
    handler.render_report(&mut output, diagnostic).unwrap();
    output
}

fn print_scenario(name: &str, description: &str) {
    println!();
    println!("{}", "═".repeat(78).dimmed());
    println!("{} {}", "SCENARIO:".bold().cyan(), name.bold().white());
    println!("{}", "─".repeat(78).dimmed());
    println!("{}", description.dimmed());
    println!("{}", "═".repeat(78).dimmed());
}

fn print_kdl(kdl: &str) {
    println!();
    println!("{}", "KDL Input:".bold().green());
    println!("{}", "─".repeat(60).dimmed());

    for (i, line) in kdl.lines().enumerate() {
        println!(
            "{} {} {}",
            format!("{:3}", i + 1).dimmed(),
            "│".dimmed(),
            line
        );
    }
    println!("{}", "─".repeat(60).dimmed());
}

/// Format a variant name with colors: EnumName::VariantName
/// EnumName in cyan, VariantName in yellow
fn colored_variant(name: &str) -> String {
    if let Some((enum_part, variant_part)) = name.split_once("::") {
        format!("{}::{}", enum_part.cyan(), variant_part.yellow())
    } else {
        name.yellow().to_string()
    }
}

/// Compute character-level diff between two strings
/// Returns (wrong_highlighted, correct_highlighted) with ANSI colors
fn char_diff(wrong: &str, correct: &str) -> (String, String) {
    // Use simple LCS-based diff
    let wrong_chars: Vec<char> = wrong.chars().collect();
    let correct_chars: Vec<char> = correct.chars().collect();

    // Build LCS table
    let m = wrong_chars.len();
    let n = correct_chars.len();
    let mut dp = vec![vec![0usize; n + 1]; m + 1];

    for i in 1..=m {
        for j in 1..=n {
            if wrong_chars[i - 1] == correct_chars[j - 1] {
                dp[i][j] = dp[i - 1][j - 1] + 1;
            } else {
                dp[i][j] = dp[i - 1][j].max(dp[i][j - 1]);
            }
        }
    }

    // Backtrack to find which chars are common
    let mut wrong_common = vec![false; m];
    let mut correct_common = vec![false; n];
    let mut i = m;
    let mut j = n;

    while i > 0 && j > 0 {
        if wrong_chars[i - 1] == correct_chars[j - 1] {
            wrong_common[i - 1] = true;
            correct_common[j - 1] = true;
            i -= 1;
            j -= 1;
        } else if dp[i - 1][j] > dp[i][j - 1] {
            i -= 1;
        } else {
            j -= 1;
        }
    }

    // Build highlighted strings
    let mut wrong_result = String::new();
    for (idx, ch) in wrong_chars.iter().enumerate() {
        if wrong_common[idx] {
            wrong_result.push_str(&format!("{}", ch.to_string().dimmed()));
        } else {
            wrong_result.push_str(&format!("{}", ch.to_string().red().strikethrough()));
        }
    }

    let mut correct_result = String::new();
    for (idx, ch) in correct_chars.iter().enumerate() {
        if correct_common[idx] {
            correct_result.push_str(&format!("{}", ch.to_string().dimmed()));
        } else {
            correct_result.push_str(&format!("{}", ch.to_string().green().bold()));
        }
    }

    (wrong_result, correct_result)
}

/// Generate help text from a SolverError - ALL FROM REAL DATA!
/// When `include_suggestions` is false, suggestions are omitted (shown as labels instead)
fn help_from_solver_error(err: &SolverError, include_suggestions: bool) -> String {
    match err {
        SolverError::Ambiguous {
            candidates,
            disambiguating_fields,
        } => {
            let colored_candidates: Vec<_> =
                candidates.iter().map(|c| colored_variant(c)).collect();

            let mut help = format!(
                "Multiple variants match: {}\n",
                colored_candidates.join(", ")
            );
            if !disambiguating_fields.is_empty() {
                let colored_fields: Vec<_> = disambiguating_fields
                    .iter()
                    .map(|f| f.green().to_string())
                    .collect();
                help.push_str(&format!(
                    "Add one of these fields to disambiguate: {}",
                    colored_fields.join(", ")
                ));
            } else {
                // No disambiguating fields - suggest using KDL type annotation
                help.push_str("Use a KDL type annotation to specify the variant:\n");
                for candidate in candidates {
                    // Extract just the variant name from "EnumName::VariantName"
                    let variant_name = candidate.split("::").last().unwrap_or(candidate);
                    help.push_str(&format!(
                        "  {}node-name {} ...\n",
                        format!("({variant_name})").cyan(),
                        "\"arg\"".dimmed()
                    ));
                }
            }
            help
        }
        SolverError::NoMatch {
            candidate_failures,
            suggestions,
            ..
        } => {
            let mut help = String::new();

            // Check if there's a clear "best" candidate (first one after sorting has more matches)
            let best_candidate = candidate_failures.first();
            let second_best = candidate_failures.get(1);

            let has_clear_winner = match (best_candidate, second_best) {
                (Some(best), Some(second)) => best.suggestion_matches > second.suggestion_matches,
                (Some(best), None) => best.suggestion_matches > 0,
                _ => false,
            };

            if has_clear_winner {
                let best = best_candidate.unwrap();
                help.push_str(&format!(
                    "Did you mean {}?\n\n",
                    colored_variant(&best.variant_name).bold()
                ));
            }

            // Show why each candidate failed
            if !candidate_failures.is_empty() {
                if has_clear_winner {
                    help.push_str("All variants checked:\n");
                } else {
                    help.push_str("No variant matched:\n");
                }
                for failure in candidate_failures {
                    help.push_str(&format!("  • {}", colored_variant(&failure.variant_name)));

                    if !failure.missing_fields.is_empty() {
                        let colored_missing: Vec<_> = failure
                            .missing_fields
                            .iter()
                            .map(|m| m.name.red().to_string())
                            .collect();
                        help.push_str(&format!(": missing {}", colored_missing.join(", ")));
                    }
                    if !failure.unknown_fields.is_empty() {
                        if failure.missing_fields.is_empty() {
                            help.push(':');
                        } else {
                            help.push(',');
                        }
                        let colored_unknown: Vec<_> = failure
                            .unknown_fields
                            .iter()
                            .map(|f| f.yellow().to_string())
                            .collect();
                        help.push_str(&format!(" unexpected {}", colored_unknown.join(", ")));
                    }
                    help.push('\n');
                }
            }

            // Show "did you mean?" suggestions with character-level diff (if requested)
            if include_suggestions && !suggestions.is_empty() {
                help.push('\n');
                for suggestion in suggestions {
                    let (wrong_diff, correct_diff) =
                        char_diff(&suggestion.unknown, suggestion.suggestion);
                    help.push_str(&format!(
                        "  {} → {}  (did you mean {}?)\n",
                        wrong_diff,
                        correct_diff,
                        suggestion.suggestion.green(),
                    ));
                }
            }

            help
        }
    }
}

/// Build labels for "did you mean?" suggestions pointing to exact locations
fn build_suggestion_labels(kdl: &str, err: &SolverError) -> Vec<((usize, usize), String)> {
    let mut labels = Vec::new();

    if let SolverError::NoMatch { suggestions, .. } = err {
        for suggestion in suggestions {
            if let Some((start, end)) = find_property_span(kdl, &suggestion.unknown) {
                let label = format!("did you mean `{}`?", suggestion.suggestion);
                labels.push(((start, end - start), label));
            }
        }
    }

    labels
}

/// Extract solver error from KdlError if present
fn extract_solver_error(err: &KdlError) -> Option<&SolverError> {
    match err.kind() {
        KdlErrorKind::Solver(solver_err) => Some(solver_err),
        _ => None,
    }
}

/// Generate a SHORT error message (for the diagnostic title)
fn short_error_message(err: &SolverError) -> String {
    match err {
        SolverError::Ambiguous { candidates, .. } => {
            format!("ambiguous: {} variants match", candidates.len())
        }
        SolverError::NoMatch {
            candidate_failures, ..
        } => {
            format!("no match: {} variants checked", candidate_failures.len())
        }
    }
}

// ============================================================================
// Error Scenarios
// ============================================================================

fn scenario_ambiguous_enum() {
    print_scenario(
        "Ambiguous Flattened Enum",
        "Both TypeA and TypeB variants have identical fields (value, priority).\n\
         The solver cannot determine which variant to use.",
    );

    let kdl = r#"resource "test" value="hello" priority=10"#;
    print_kdl(kdl);

    let result: Result<AmbiguousConfig, _> = facet_kdl::from_str(kdl);

    match result {
        Ok(_) => println!("\nUnexpected success!"),
        Err(e) => {
            // Generate short message and detailed help from REAL error data
            let (message, help) = if let Some(solver_err) = extract_solver_error(&e) {
                (
                    short_error_message(solver_err),
                    help_from_solver_error(solver_err, true), // include suggestions in help
                )
            } else {
                (format!("{e}"), "(non-solver error)".to_string())
            };

            let diagnostic = KdlDiagnostic::new("config.kdl", kdl, message)
                .with_label(0..kdl.len(), "error occurred here")
                .with_help(help);

            println!("\n{}", "Rich Diagnostic:".bold().yellow());
            println!("{}", render_error(&diagnostic));
        }
    }
}

fn scenario_no_match_with_failures() {
    print_scenario(
        "NoMatch with Per-Candidate Failures",
        "Provide field names that don't exactly match any variant.\n\
         The solver will show WHY each candidate failed!",
    );

    // Use misspelled field names that trigger NoMatch
    // Redis needs: host, port, password
    // Postgres needs: connection_string, pool_size
    // We provide: 'hst' (typo for host), 'conn_str' (typo for connection_string)
    let kdl = r#"backend "cache" hst="localhost" conn_str="pg""#;
    print_kdl(kdl);

    let result: Result<NoMatchConfig, _> = facet_kdl::from_str(kdl);

    match result {
        Ok(config) => {
            println!("\n{} {}", "Success:".bold().green(), config.pretty());
        }
        Err(e) => {
            let (message, help, labels) = if let Some(solver_err) = extract_solver_error(&e) {
                (
                    short_error_message(solver_err),
                    help_from_solver_error(solver_err, false), // DON'T include suggestions (shown as labels)
                    build_suggestion_labels(kdl, solver_err),
                )
            } else {
                (format!("{e}"), "(non-solver error)".to_string(), vec![])
            };

            let mut diagnostic = KdlDiagnostic::new("config.kdl", kdl, message);

            // Add labels pointing to each typo
            for ((start, len), label) in labels {
                diagnostic = diagnostic.with_label(start..(start + len), label);
            }

            diagnostic = diagnostic.with_help(help);

            println!("\n{}", "Rich Diagnostic:".bold().yellow());
            println!("{}", render_error(&diagnostic));
        }
    }
}

fn scenario_typo_suggestions() {
    print_scenario(
        "Unknown Fields with 'Did You Mean?' Suggestions",
        "Misspell field names and see the solver suggest corrections!\n\
         Uses Jaro-Winkler similarity to find close matches.",
    );

    // Typos: 'hostnam' instead of 'hostname', 'prot' instead of 'port'
    let kdl = r#"server "web" hostnam="localhost" prot=8080"#;
    print_kdl(kdl);

    let result: Result<TypoConfig, _> = facet_kdl::from_str(kdl);

    match result {
        Ok(_) => println!("\nUnexpected success!"),
        Err(e) => {
            let (message, help, labels) = if let Some(solver_err) = extract_solver_error(&e) {
                (
                    short_error_message(solver_err),
                    help_from_solver_error(solver_err, false), // DON'T include suggestions (shown as labels)
                    build_suggestion_labels(kdl, solver_err),
                )
            } else {
                (format!("{e}"), "(non-solver error)".to_string(), vec![])
            };

            let mut diagnostic = KdlDiagnostic::new("config.kdl", kdl, message);

            // Add labels pointing to each typo
            for ((start, len), label) in labels {
                diagnostic = diagnostic.with_label(start..(start + len), label);
            }

            diagnostic = diagnostic.with_help(help);

            println!("\n{}", "Rich Diagnostic:".bold().yellow());
            println!("{}", render_error(&diagnostic));
        }
    }
}

fn scenario_value_overflow() {
    print_scenario(
        "Value Overflow Detection",
        "When a value doesn't fit ANY candidate type, the solver reports it.\n\
         count=5000000000 exceeds both u8 (max 255) and u32 (max ~4 billion).",
    );

    // Too large - doesn't fit any type!
    let kdl_overflow = r#"data count=5000000000"#;
    print_kdl(kdl_overflow);

    let result: Result<ValueConfig, _> = facet_kdl::from_str(kdl_overflow);
    match result {
        Ok(_) => println!("\nUnexpected success!"),
        Err(e) => {
            println!("\n{}", "Error (raw):".bold().red());
            println!("  {e}");
            println!("\n(This is a real error from facet-kdl's value-based disambiguation!)");
        }
    }
}

fn scenario_multiline() {
    print_scenario(
        "Multi-Line Config with Typos",
        "A more realistic multi-line configuration file with several typos.\n\
         Shows how the solver sorts candidates by closeness to the input.",
    );

    // Multi-line KDL with typos that look like MySQL:
    // - 'hots' instead of 'host'
    // - 'prot' instead of 'port'
    // - 'usernme' instead of 'username'
    // - 'pasword' instead of 'password'
    // Note: properties go on the same line as the node in KDL
    let kdl = r#"database "production" \
    hots="db.example.com" \
    prot=3306 \
    usernme="admin" \
    pasword="secret123"
"#;
    print_kdl(kdl);

    let result: Result<MultiLineConfig, _> = facet_kdl::from_str(kdl);

    match result {
        Ok(config) => {
            println!("\n{} {}", "Success:".bold().green(), config.pretty());
        }
        Err(e) => {
            let (message, help, labels) = if let Some(solver_err) = extract_solver_error(&e) {
                (
                    short_error_message(solver_err),
                    help_from_solver_error(solver_err, false),
                    build_suggestion_labels(kdl, solver_err),
                )
            } else {
                (format!("{e}"), "(non-solver error)".to_string(), vec![])
            };

            let mut diagnostic = KdlDiagnostic::new("database.kdl", kdl, message);

            // Add labels pointing to each typo
            for ((start, len), label) in labels {
                diagnostic = diagnostic.with_label(start..(start + len), label);
            }

            diagnostic = diagnostic.with_help(help);

            println!("\n{}", "Rich Diagnostic:".bold().yellow());
            println!("{}", render_error(&diagnostic));
        }
    }
}

// ============================================================================
// Main
// ============================================================================

fn main() {
    println!();
    let header = boxen::builder()
        .border_style(BorderStyle::Round)
        .border_color("cyan")
        .text_alignment(TextAlignment::Center)
        .padding(1)
        .render("FACET-KDL ERROR SHOWCASE\n\nALL help text generated from REAL error data!\nNo hardcoded messages - what you see is what facet-solver provides.")
        .unwrap();
    println!("{header}");

    scenario_ambiguous_enum();
    scenario_no_match_with_failures();
    scenario_typo_suggestions();
    scenario_value_overflow();
    scenario_multiline();

    println!();
    let footer = boxen::builder()
        .border_style(BorderStyle::Round)
        .border_color("green")
        .text_alignment(TextAlignment::Center)
        .padding(1)
        .render("END OF SHOWCASE\n\nAll diagnostics above were generated from REAL facet-solver errors!")
        .unwrap();
    println!("{footer}");
}
