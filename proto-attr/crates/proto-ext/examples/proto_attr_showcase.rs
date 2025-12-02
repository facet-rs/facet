//! Showcase of proto-attr compile-time error messages
//!
//! This example demonstrates the helpful error messages you get when
//! using extension attributes incorrectly.
//!
//! Run with: cargo run --example proto_attr_showcase

use facet_showcase::{Language, ShowcaseRunner};
use std::process::Command;

fn main() {
    let mut runner = ShowcaseRunner::new("Proto-Attr").language(Language::Rust);
    runner.header();

    // Basic attribute errors
    scenario_unknown_attribute(&mut runner);
    scenario_typo_skip(&mut runner);
    scenario_skip_with_args(&mut runner);
    scenario_rename_missing_value(&mut runner);
    scenario_column_unknown_field(&mut runner);
    scenario_column_name_missing_value(&mut runner);

    // Index attribute errors (list_string field type)
    scenario_index_typo_columns(&mut runner);
    scenario_index_wrong_type(&mut runner);

    // Range attribute errors (opt_i64 field type)
    scenario_range_wrong_type(&mut runner);

    // OnDelete attribute errors (ident field type)
    scenario_on_delete_string_instead_of_ident(&mut runner);

    // Advanced errors
    scenario_duplicate_field(&mut runner);
    scenario_mixed_types_in_list(&mut runner);
    scenario_wrong_bracket_type(&mut runner);
    scenario_integer_overflow(&mut runner);
    scenario_bool_as_string(&mut runner);
    scenario_integer_used_as_flag(&mut runner);

    // Smart suggestions for common mistakes
    scenario_ident_instead_of_string(&mut runner);
    scenario_single_string_instead_of_list(&mut runner);

    // Help text in error messages
    scenario_help_text_column(&mut runner);
    scenario_help_text_index(&mut runner);
    scenario_help_text_range(&mut runner);

    // Valid usage showcasing all field types
    scenario_valid_usage(&mut runner);

    // Top-level annotations scenarios
    top_level_annotations(&mut runner);

    runner.footer();
}

/// Compiles a test snippet and returns the compiler error output.
fn compile_snippet(code: &str) -> String {
    use std::fs;
    use std::path::Path;

    let test_dir = Path::new("/tmp/proto-attr-compile-error-test");
    let src_dir = test_dir.join("src");

    fs::create_dir_all(&src_dir).unwrap();

    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let proto_attr_crates = Path::new(manifest_dir).parent().unwrap();
    let proto_attr_path = proto_attr_crates.join("proto-attr");
    let proto_ext_path = proto_attr_crates.join("proto-ext");

    // Check if we're running with the nightly feature
    let nightly_feature = cfg!(feature = "nightly");

    let features = if nightly_feature {
        ", features = [\"nightly\"]"
    } else {
        ""
    };

    fs::write(
        test_dir.join("Cargo.toml"),
        format!(
            r#"[package]
name = "test"
version = "0.1.0"
edition = "2024"

[dependencies]
proto-attr = {{ path = "{}"{} }}
proto-ext = {{ path = "{}" }}
"#,
            proto_attr_path.display(),
            features,
            proto_ext_path.display()
        ),
    )
    .unwrap();

    fs::write(src_dir.join("main.rs"), code).unwrap();

    let output = Command::new("cargo")
        .args(["check", "--color=always"])
        .current_dir(test_dir)
        .env("CARGO_TERM_COLOR", "always")
        .output()
        .expect("Failed to run cargo check");

    String::from_utf8_lossy(&output.stderr).to_string()
}

/// Extract just the error message from cargo output.
fn extract_error(output: &str) -> String {
    let mut lines: Vec<&str> = Vec::new();
    let mut in_error = false;

    for line in output.lines() {
        if line.contains("Compiling")
            || line.contains("Checking")
            || line.contains("Updating")
            || line.contains("Locking")
            || line.contains("Downloading")
            || line.contains("Downloaded")
        {
            continue;
        }

        if line.contains("error") {
            in_error = true;
        }

        if in_error {
            lines.push(line);
        }
    }

    lines.join("\n")
}

fn scenario_unknown_attribute(runner: &mut ShowcaseRunner) {
    let code = r#"use proto_attr::Faket;

#[derive(Faket)]
struct User {
    #[faket(proto_ext::indexed)]
    id: i64,
}

fn main() {}
"#;

    let output = compile_snippet(code);
    let error = extract_error(&output);

    runner
        .scenario("Unknown Extension Attribute")
        .description(
            "Using an unknown ORM attribute like `indexed` produces a clear error\n\
             listing all available attributes (skip, rename, column).",
        )
        .input(Language::Rust, code)
        .compiler_error(&error)
        .finish();
}

fn scenario_typo_skip(runner: &mut ShowcaseRunner) {
    let code = r#"use proto_attr::Faket;

#[derive(Faket)]
struct User {
    #[faket(proto_ext::skp)]
    password_hash: String,
}

fn main() {}
"#;

    let output = compile_snippet(code);
    let error = extract_error(&output);

    runner
        .scenario("Typo in Attribute Name")
        .description(
            "Common typos like `skp` instead of `skip` are caught at compile time\n\
             with a helpful \"did you mean?\" suggestion.",
        )
        .input(Language::Rust, code)
        .compiler_error(&error)
        .finish();
}

fn scenario_skip_with_args(runner: &mut ShowcaseRunner) {
    let code = r#"use proto_attr::Faket;

#[derive(Faket)]
struct User {
    #[faket(proto_ext::skip("serialization"))]
    password_hash: String,
}

fn main() {}
"#;

    let output = compile_snippet(code);
    let error = extract_error(&output);

    runner
        .scenario("Unit Attribute with Arguments")
        .description(
            "The `skip` attribute is a unit variant that takes no arguments.\n\
             Passing arguments produces a clear error explaining the correct usage.",
        )
        .input(Language::Rust, code)
        .compiler_error(&error)
        .finish();
}

fn scenario_rename_missing_value(runner: &mut ShowcaseRunner) {
    let code = r#"use proto_attr::Faket;

#[derive(Faket)]
#[faket(proto_ext::rename)]
struct UserProfile {
    email: String,
}

fn main() {}
"#;

    let output = compile_snippet(code);
    let error = extract_error(&output);

    runner
        .scenario("Newtype Attribute Missing Value")
        .description(
            "The `rename` attribute requires a string value to specify the new name.\n\
             Omitting the value produces an error showing the expected syntax.",
        )
        .input(Language::Rust, code)
        .compiler_error(&error)
        .finish();
}

fn scenario_column_unknown_field(runner: &mut ShowcaseRunner) {
    let code = r#"use proto_attr::Faket;

#[derive(Faket)]
struct User {
    #[faket(proto_ext::column(nam = "user_id", primary_key))]
    id: i64,
}

fn main() {}
"#;

    let output = compile_snippet(code);
    let error = extract_error(&output);

    runner
        .scenario("Unknown Field in Struct Attribute")
        .description(
            "Typos in field names like `nam` instead of `name` are caught\n\
             with a \"did you mean?\" suggestion and list of valid fields\n\
             (name, nullable, sql_type, primary_key, auto_increment).",
        )
        .input(Language::Rust, code)
        .compiler_error(&error)
        .finish();
}

fn scenario_column_name_missing_value(runner: &mut ShowcaseRunner) {
    let code = r#"use proto_attr::Faket;

#[derive(Faket)]
struct User {
    #[faket(proto_ext::column(name, primary_key))]
    id: i64,
}

fn main() {}
"#;

    let output = compile_snippet(code);
    let error = extract_error(&output);

    runner
        .scenario("Struct Field Missing Value")
        .description(
            "The `name` field in `column` requires a string value.\n\
             Using it as a flag produces an error showing the correct syntax.",
        )
        .input(Language::Rust, code)
        .compiler_error(&error)
        .finish();
}

fn scenario_index_typo_columns(runner: &mut ShowcaseRunner) {
    let code = r#"use proto_attr::Faket;

#[derive(Faket)]
#[faket(proto_ext::index(column = ["id", "email"]))]
struct UserIndex {
    id: i64,
    email: String,
}

fn main() {}
"#;

    let output = compile_snippet(code);
    let error = extract_error(&output);

    runner
        .scenario("Index Field Typo (list_string)")
        .description(
            "Typos in field names like `column` instead of `columns` are caught\n\
             with a helpful \"did you mean?\" suggestion.",
        )
        .input(Language::Rust, code)
        .compiler_error(&error)
        .finish();
}

fn scenario_index_wrong_type(runner: &mut ShowcaseRunner) {
    let code = r#"use proto_attr::Faket;

#[derive(Faket)]
#[faket(proto_ext::index(columns = "email"))]
struct UserIndex {
    id: i64,
    email: String,
}

fn main() {}
"#;

    let output = compile_snippet(code);
    let error = extract_error(&output);

    runner
        .scenario("Index Wrong Type for List")
        .description(
            "The `columns` field expects a list like `[\"a\", \"b\"]`, not a string.\n\
             Using the wrong type produces a clear error explaining the expected format.",
        )
        .input(Language::Rust, code)
        .compiler_error(&error)
        .finish();
}

fn scenario_range_wrong_type(runner: &mut ShowcaseRunner) {
    let code = r#"use proto_attr::Faket;

#[derive(Faket)]
struct User {
    #[faket(proto_ext::range(min = "zero", max = 100))]
    age: i32,
}

fn main() {}
"#;

    let output = compile_snippet(code);
    let error = extract_error(&output);

    runner
        .scenario("Range Wrong Type for Integer")
        .description(
            "The `min` and `max` fields in `range` expect integers, not strings.\n\
             Using a string produces an error showing the correct syntax.",
        )
        .input(Language::Rust, code)
        .compiler_error(&error)
        .finish();
}

fn scenario_on_delete_string_instead_of_ident(runner: &mut ShowcaseRunner) {
    let code = r#"use proto_attr::Faket;

#[derive(Faket)]
struct Post {
    #[faket(proto_ext::on_delete(action = "cascade"))]
    author_id: i64,
}

fn main() {}
"#;

    let output = compile_snippet(code);
    let error = extract_error(&output);

    runner
        .scenario("OnDelete String Instead of Identifier")
        .description(
            "The `action` field expects a bare identifier like `cascade`, not a string.\n\
             The error message suggests removing the quotes: `action = cascade`.",
        )
        .input(Language::Rust, code)
        .compiler_error(&error)
        .finish();
}

// ============================================================================
// ADVANCED ERROR SCENARIOS
// ============================================================================

fn scenario_duplicate_field(runner: &mut ShowcaseRunner) {
    let code = r#"use proto_attr::Faket;

#[derive(Faket)]
struct User {
    #[faket(proto_ext::column(name = "user_id", name = "id"))]
    id: i64,
}

fn main() {}
"#;

    let output = compile_snippet(code);
    let error = extract_error(&output);

    runner
        .scenario("Duplicate Field")
        .description(
            "Specifying the same field twice in an attribute is an error.\n\
             Each field can only appear once in an attribute.",
        )
        .input(Language::Rust, code)
        .compiler_error(&error)
        .finish();
}

fn scenario_mixed_types_in_list(runner: &mut ShowcaseRunner) {
    let code = r#"use proto_attr::Faket;

#[derive(Faket)]
#[faket(proto_ext::index(columns = ["email", 123]))]
struct UserIndex {
    id: i64,
    email: String,
}

fn main() {}
"#;

    let output = compile_snippet(code);
    let error = extract_error(&output);

    runner
        .scenario("Mixed Types in List")
        .description(
            "List fields require all elements to be the same type.\n\
             A string list like `columns` cannot contain integers.",
        )
        .input(Language::Rust, code)
        .compiler_error(&error)
        .finish();
}

fn scenario_wrong_bracket_type(runner: &mut ShowcaseRunner) {
    let code = r#"use proto_attr::Faket;

#[derive(Faket)]
#[faket(proto_ext::index(columns = {"email"}))]
struct UserIndex {
    id: i64,
    email: String,
}

fn main() {}
"#;

    let output = compile_snippet(code);
    let error = extract_error(&output);

    runner
        .scenario("Wrong Bracket Type for List")
        .description(
            "Lists use square brackets `[...]`, not curly braces `{...}`.\n\
             The error specifically tells you to use square brackets.",
        )
        .input(Language::Rust, code)
        .compiler_error(&error)
        .finish();
}

fn scenario_integer_overflow(runner: &mut ShowcaseRunner) {
    let code = r#"use proto_attr::Faket;

#[derive(Faket)]
struct User {
    #[faket(proto_ext::range(min = 99999999999999999999999))]
    score: i32,
}

fn main() {}
"#;

    let output = compile_snippet(code);
    let error = extract_error(&output);

    runner
        .scenario("Integer Overflow")
        .description(
            "The error shows the field name, the value, and the schema-defined type.\n\
             Each integer field in the grammar specifies its type (here: i64).",
        )
        .input(Language::Rust, code)
        .compiler_error(&error)
        .finish();
}

fn scenario_bool_as_string(runner: &mut ShowcaseRunner) {
    let code = r#"use proto_attr::Faket;

#[derive(Faket)]
struct User {
    #[faket(proto_ext::column(primary_key = "true"))]
    id: i64,
}

fn main() {}
"#;

    let output = compile_snippet(code);
    let error = extract_error(&output);

    runner
        .scenario("Bool Field with String Value")
        .description(
            "Boolean fields expect `true` or `false` literals, not strings.\n\
             The error suggests removing the quotes: `primary_key = true`.",
        )
        .input(Language::Rust, code)
        .compiler_error(&error)
        .finish();
}

fn scenario_integer_used_as_flag(runner: &mut ShowcaseRunner) {
    let code = r#"use proto_attr::Faket;

#[derive(Faket)]
struct User {
    #[faket(proto_ext::range(min, max = 100))]
    age: i32,
}

fn main() {}
"#;

    let output = compile_snippet(code);
    let error = extract_error(&output);

    runner
        .scenario("Integer Field Used as Flag")
        .description(
            "Integer fields require a value; they cannot be used as flags.\n\
             Using `min` without `= value` produces an error.",
        )
        .input(Language::Rust, code)
        .compiler_error(&error)
        .finish();
}

// ============================================================================
// SMART SUGGESTIONS FOR COMMON MISTAKES
// ============================================================================

fn scenario_ident_instead_of_string(runner: &mut ShowcaseRunner) {
    let code = r#"use proto_attr::Faket;

#[derive(Faket)]
struct User {
    #[faket(proto_ext::column(name = user_id))]
    id: i64,
}

fn main() {}
"#;

    let output = compile_snippet(code);
    let error = extract_error(&output);

    runner
        .scenario("Identifier Instead of String")
        .description(
            "String fields require quoted values, not bare identifiers.\n\
             The error suggests adding quotes: `name = \"user_id\"`.",
        )
        .input(Language::Rust, code)
        .compiler_error(&error)
        .finish();
}

fn scenario_single_string_instead_of_list(runner: &mut ShowcaseRunner) {
    let code = r#"use proto_attr::Faket;

#[derive(Faket)]
#[faket(proto_ext::index(columns = "email"))]
struct UserIndex {
    id: i64,
    email: String,
}

fn main() {}
"#;

    let output = compile_snippet(code);
    let error = extract_error(&output);

    runner
        .scenario("Single String Instead of List")
        .description(
            "List fields require `[...]` syntax even for a single element.\n\
             The error suggests wrapping in brackets: `columns = [\"email\"]`.",
        )
        .input(Language::Rust, code)
        .compiler_error(&error)
        .finish();
}

// ============================================================================
// HELP TEXT IN ERROR MESSAGES
// ============================================================================

fn scenario_help_text_column(runner: &mut ShowcaseRunner) {
    let code = r#"use proto_attr::Faket;

#[derive(Faket)]
struct User {
    #[faket(proto_ext::column(primary_key = "yes"))]
    id: i64,
}

fn main() {}
"#;

    let output = compile_snippet(code);
    let error = extract_error(&output);

    runner
        .scenario("Help Text: Column Primary Key")
        .description(
            "Error messages include contextual help explaining the field AND how to use it.\n\
             The help shows: correct syntax, typical usage, and semantic meaning.",
        )
        .input(Language::Rust, code)
        .compiler_error(&error)
        .finish();
}

fn scenario_help_text_index(runner: &mut ShowcaseRunner) {
    let code = r#"use proto_attr::Faket;

#[derive(Faket)]
#[faket(proto_ext::index(columns))]
struct UserIndex {
    id: i64,
    email: String,
}

fn main() {}
"#;

    let output = compile_snippet(code);
    let error = extract_error(&output);

    runner
        .scenario("Help Text: Index Columns")
        .description(
            "The help text explains that `columns` specifies which columns\n\
             to include in the index: \"Columns to include in the index\".",
        )
        .input(Language::Rust, code)
        .compiler_error(&error)
        .finish();
}

fn scenario_help_text_range(runner: &mut ShowcaseRunner) {
    let code = r#"use proto_attr::Faket;

#[derive(Faket)]
struct User {
    #[faket(proto_ext::range(min = "zero"))]
    age: i32,
}

fn main() {}
"#;

    let output = compile_snippet(code);
    let error = extract_error(&output);

    runner
        .scenario("Help Text: Range Min")
        .description(
            "The help text clarifies that `min` is the \"Minimum value (inclusive)\".\n\
             Doc comments in the grammar DSL become contextual help in errors.",
        )
        .input(Language::Rust, code)
        .compiler_error(&error)
        .finish();
}

fn scenario_valid_usage(runner: &mut ShowcaseRunner) {
    let code = r#"use proto_attr::Faket;

/// A table we want to exclude from ORM generation
#[derive(Faket)]
#[faket(proto_ext::skip)]
struct InternalCache {
    data: Vec<u8>,
}

/// Map to a different table name
#[derive(Faket)]
#[faket(proto_ext::rename("user_profiles"))]
struct UserProfile {
    email: String,
}

/// Full ORM column configuration example
#[derive(Faket)]
#[faket(proto_ext::index(name = "idx_user_email", columns = ["email"], unique))]
struct User {
    /// Primary key with auto-increment
    #[faket(proto_ext::column(name = "id", primary_key, auto_increment))]
    id: i64,

    /// Custom column name
    #[faket(proto_ext::column(name = "user_name"))]
    name: String,

    /// Nullable TEXT field for bio
    #[faket(proto_ext::column(nullable, sql_type = "TEXT"))]
    bio: Option<String>,

    /// Non-nullable timestamp
    #[faket(proto_ext::column(nullable = false, sql_type = "TIMESTAMP"))]
    created_at: i64,

    /// Skip sensitive field from serialization
    #[faket(proto_ext::skip)]
    password_hash: String,

    /// Rename field for API compatibility
    #[faket(proto_ext::rename("email_address"))]
    email: String,

    /// Validation: age must be between 0 and 150
    #[faket(proto_ext::range(min = 0, max = 150, message = "Age must be realistic"))]
    age: i32,
}

/// Foreign key with ON DELETE behavior
#[derive(Faket)]
struct Post {
    #[faket(proto_ext::column(primary_key, auto_increment))]
    id: i64,

    /// When author is deleted, cascade delete their posts
    #[faket(proto_ext::on_delete(action = cascade))]
    author_id: i64,

    /// When category is deleted, set to null
    #[faket(proto_ext::on_delete(action = set_null))]
    category_id: Option<i64>,

    title: String,
}

/// Composite index example
#[derive(Faket)]
#[faket(proto_ext::index(columns = ["user_id", "created_at"]))]
#[faket(proto_ext::index(name = "idx_status", columns = ["status"], unique))]
struct Order {
    #[faket(proto_ext::column(primary_key))]
    id: i64,
    user_id: i64,
    status: String,
    created_at: i64,
}

fn main() {
    println!("Compiles successfully!");
}
"#;

    let output = compile_snippet(code);

    let has_error = output.contains("error[") || output.contains("error:");
    let error_output = if has_error {
        extract_error(&output)
    } else {
        "✓ Compilation successful! No errors.".to_string()
    };

    runner
        .scenario("Valid Usage")
        .description(
            "When ORM attributes are used correctly, everything compiles smoothly.\n\
             This shows realistic usage patterns:\n\
             • skip - exclude structs/fields from generation\n\
             • rename - map to different table/column names\n\
             • column - full control: name, nullable, sql_type, primary_key, auto_increment\n\
             • index - database indexes with columns list (list_string field type)\n\
             • range - validation bounds with min/max (opt_i64 field type)\n\
             • on_delete - foreign key behavior with bare identifiers (ident field type)",
        )
        .input(Language::Rust, code)
        .compiler_error(&error_output)
        .finish();
}

fn top_level_annotations(runner: &mut ShowcaseRunner) {
    // Scenario 1: Basic top-level attributes
    let code = r#"use proto_attr::Faket;

/// A newtype wrapper (transparent)
#[derive(Faket)]
#[faket(transparent)]
struct UserId(i64);

/// An untagged enum
#[derive(Faket)]
#[faket(untagged)]
enum Message {
    Text(String),
    Number(i64),
}

/// Struct with rename_all for field case conversion
#[derive(Faket)]
#[faket(rename_all = "kebab-case")]
struct Config {
    api_key: String,
    max_retries: i32,
}

fn main() {}
"#;

    let output = compile_snippet(code);
    let error_output = if output.contains("error[") || output.contains("error:") {
        extract_error(&output)
    } else {
        "✓ Compilation successful! No errors.".to_string()
    };

    runner
        .scenario("Top-Level: Basic Attributes")
        .description(
            "Top-level facet attributes like `transparent`, `untagged`, and `rename_all`\n\
             work without a namespace prefix.",
        )
        .input(Language::Rust, code)
        .compiler_error(&error_output)
        .finish();

    // Scenario 2: Multiple top-level attrs in one #[faket(...)]
    let code = r#"use proto_attr::Faket;

/// Tagged enum with custom tag and content field names
#[derive(Faket)]
#[faket(tag = "type", content = "payload")]
enum Event {
    Click { x: i32, y: i32 },
    KeyPress(String),
}

/// Struct with multiple top-level attrs
#[derive(Faket)]
#[faket(deny_unknown_fields, default, rename_all = "camelCase")]
struct ApiRequest {
    user_id: i64,
    request_type: String,
}

fn main() {}
"#;

    let output = compile_snippet(code);
    let error_output = if output.contains("error[") || output.contains("error:") {
        extract_error(&output)
    } else {
        "✓ Compilation successful! No errors.".to_string()
    };

    runner
        .scenario("Top-Level: Multiple Attrs Combined")
        .description(
            "Multiple top-level attributes can be combined in a single `#[faket(...)]`.\n\
             Commas separate distinct attributes, respecting balanced parentheses.",
        )
        .input(Language::Rust, code)
        .compiler_error(&error_output)
        .finish();

    // Scenario 3: Mixing top-level and namespaced attributes
    let code = r#"use proto_attr::Faket;

/// Mix top-level facet attrs with extension attrs on a struct
#[derive(Faket)]
#[faket(rename_all = "snake_case", proto_ext::index(columns = ["id", "name"]))]
struct User {
    #[faket(proto_ext::column(primary_key, auto_increment))]
    id: i64,

    #[faket(default, proto_ext::column(name = "user_name"))]
    name: String,

    #[faket(proto_ext::skip, proto_ext::rename("ignored"))]
    internal_cache: Vec<u8>,
}

fn main() {}
"#;

    let output = compile_snippet(code);
    let error_output = if output.contains("error[") || output.contains("error:") {
        extract_error(&output)
    } else {
        "✓ Compilation successful! No errors.".to_string()
    };

    runner
        .scenario("Top-Level: Mixed with Namespaced")
        .description(
            "Top-level facet attributes and namespaced extension attributes can be\n\
             freely mixed in the same `#[faket(...)]` or on different lines.",
        )
        .input(Language::Rust, code)
        .compiler_error(&error_output)
        .finish();

    // Scenario 4: Field-level top-level attrs
    let code = r#"use proto_attr::Faket;

#[derive(Faket)]
struct Settings {
    /// Field uses default if missing
    #[faket(default)]
    timeout_ms: i64,

    /// Multiple field attrs: default + extension
    #[faket(default, proto_ext::range(min = 1, max = 100))]
    max_retries: i32,

    /// Combining untagged with column config (field-level)
    #[faket(untagged, proto_ext::column(nullable))]
    optional_data: Option<String>,
}

fn main() {}
"#;

    let output = compile_snippet(code);
    let error_output = if output.contains("error[") || output.contains("error:") {
        extract_error(&output)
    } else {
        "✓ Compilation successful! No errors.".to_string()
    };

    runner
        .scenario("Top-Level: Field-Level Attrs")
        .description(
            "Top-level attributes like `default` and `untagged` can also be used\n\
             at the field level, mixed with extension attributes.",
        )
        .input(Language::Rust, code)
        .compiler_error(&error_output)
        .finish();

    // Scenario 5: Complex real-world example
    let code = r#"use proto_attr::Faket;

/// A complete API model with all attribute types
#[derive(Faket)]
#[faket(deny_unknown_fields, rename_all = "camelCase")]
#[faket(proto_ext::index(name = "idx_api_model", columns = ["id", "created_at"], unique))]
struct ApiModel {
    #[faket(proto_ext::column(primary_key, auto_increment, name = "model_id"))]
    id: i64,

    #[faket(default, proto_ext::column(sql_type = "TIMESTAMP"))]
    created_at: i64,

    #[faket(proto_ext::rename("modelName"), proto_ext::column(name = "name"))]
    model_name: String,

    #[faket(default, proto_ext::range(min = 0, max = 1000000))]
    score: i64,

    #[faket(proto_ext::skip)]
    internal_state: Vec<u8>,

    #[faket(proto_ext::on_delete(action = cascade))]
    parent_id: Option<i64>,
}

/// Enum with adjacently tagged representation
#[derive(Faket)]
#[faket(tag = "kind", content = "data", rename_all = "SCREAMING_SNAKE_CASE")]
enum ApiResponse {
    Success { value: i64 },
    Error { code: i32, message: String },
    Pending,
}

fn main() {}
"#;

    let output = compile_snippet(code);
    let error_output = if output.contains("error[") || output.contains("error:") {
        extract_error(&output)
    } else {
        "✓ Compilation successful! No errors.".to_string()
    };

    runner
        .scenario("Top-Level: Complex Real-World Example")
        .description(
            "A complete real-world example combining:\n\
             • Struct-level: deny_unknown_fields, rename_all, index\n\
             • Field-level: default, column, range, skip, rename, on_delete\n\
             • Enum: tag, content, rename_all",
        )
        .input(Language::Rust, code)
        .compiler_error(&error_output)
        .finish();

    // Scenario 6: Error case - unknown top-level attribute
    let code = r#"use proto_attr::Faket;

#[derive(Faket)]
#[faket(unknown_attr)]
struct Bad {
    field: i32,
}

fn main() {}
"#;

    let output = compile_snippet(code);
    let error_output = extract_error(&output);

    runner
        .scenario("Top-Level: Unknown Attribute Error")
        .description(
            "Using an unknown top-level attribute produces a helpful error\n\
             with suggestions for valid attributes.",
        )
        .input(Language::Rust, code)
        .compiler_error(&error_output)
        .finish();

    // Scenario 7: Error case - wrong syntax for top-level attr
    let code = r#"use proto_attr::Faket;

#[derive(Faket)]
#[faket(transparent("value"))]
struct Bad(i64);

fn main() {}
"#;

    let output = compile_snippet(code);
    let error_output = extract_error(&output);

    runner
        .scenario("Top-Level: Wrong Syntax Error")
        .description(
            "Top-level unit attributes like `transparent` don't take arguments.\n\
             The error message explains the correct usage.",
        )
        .input(Language::Rust, code)
        .compiler_error(&error_output)
        .finish();

    // Scenario 8: Error case - missing value for newtype attr
    let code = r#"use proto_attr::Faket;

#[derive(Faket)]
#[faket(rename_all)]
struct Bad {
    field: i32,
}

fn main() {}
"#;

    let output = compile_snippet(code);
    let error_output = extract_error(&output);

    runner
        .scenario("Top-Level: Missing Value Error")
        .description(
            "Newtype attributes like `rename_all` require a value.\n\
             Omitting it produces a clear error.",
        )
        .input(Language::Rust, code)
        .compiler_error(&error_output)
        .finish();
}
