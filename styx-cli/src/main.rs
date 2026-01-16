//! Styx CLI tool
//!
//! File-first design:
//!   styx <file> [options]         - operate on a file
//!   styx @<cmd> [args] [options]  - run a subcommand

use std::io::{self, Read};
use std::path::Path;

use styx_format::{FormatOptions, format_value};
use styx_schema::{SchemaFile, validate};
use styx_tree::{Payload, Value};

// ============================================================================
// Exit codes
// ============================================================================

const EXIT_SUCCESS: i32 = 0;
const EXIT_SYNTAX_ERROR: i32 = 1;
const EXIT_VALIDATION_ERROR: i32 = 2;
const EXIT_IO_ERROR: i32 = 3;

// ============================================================================
// Main entry point
// ============================================================================

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let result = if args.is_empty() || args[0] == "--help" || args[0] == "-h" {
        print_usage();
        Ok(())
    } else if args[0] == "--version" || args[0] == "-V" {
        println!("styx {VERSION}");
        Ok(())
    } else if args[0].starts_with('@') {
        // Subcommand mode: styx @tree, styx @diff, etc.
        run_subcommand(&args[0][1..], &args[1..])
    } else {
        // File-first mode: styx <file> [options]
        run_file_first(&args)
    };

    match result {
        Ok(()) => std::process::exit(EXIT_SUCCESS),
        Err(e) => {
            match &e {
                CliError::ParseDiagnostic {
                    error,
                    source,
                    filename,
                } => {
                    // Render pretty diagnostic
                    if let Some(parse_error) = error.as_parse_error() {
                        parse_error.write_report(filename, source, std::io::stderr());
                    } else {
                        eprintln!("error: {e}");
                    }
                }
                _ => {
                    eprintln!("error: {e}");
                }
            }
            std::process::exit(e.exit_code());
        }
    }
}

fn print_usage() {
    eprintln!(
        r#"styx - command-line tool for Styx configuration files

USAGE:
    styx <file> [options]           Process a Styx file
    styx @<command> [args]          Run a subcommand

FILE MODE OPTIONS:
    -o <file>                       Output to file (styx format)
    --json-out <file>               Output as JSON
    --in-place                      Modify input file in place
    --compact                       Single-line formatting
    --validate                      Validate against declared schema
    --override-schema <file>        Use this schema instead of declared

SUBCOMMANDS:
    @tree <file>                    Show debug parse tree
    @diff <old> <new>               Structural diff (not yet implemented)
    @lsp                            Start language server (stdio)

EXAMPLES:
    styx config.styx                Format and print to stdout
    styx config.styx --in-place     Format file in place
    styx config.styx --json-out -   Convert to JSON, print to stdout
    styx - < input.styx             Read from stdin
    styx @tree config.styx          Show parse tree
"#
    );
}

// ============================================================================
// Error handling
// ============================================================================

#[derive(Debug)]
#[allow(dead_code)]
enum CliError {
    Io(io::Error),
    Parse(String),
    /// Parse error with source and filename for pretty diagnostics
    ParseDiagnostic {
        error: styx_tree::BuildError,
        source: String,
        filename: String,
    },
    Validation(String),
    Usage(String),
}

impl CliError {
    fn exit_code(&self) -> i32 {
        match self {
            CliError::Io(_) => EXIT_IO_ERROR,
            CliError::Parse(_) => EXIT_SYNTAX_ERROR,
            CliError::ParseDiagnostic { .. } => EXIT_SYNTAX_ERROR,
            CliError::Validation(_) => EXIT_VALIDATION_ERROR,
            CliError::Usage(_) => EXIT_SYNTAX_ERROR,
        }
    }
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CliError::Io(e) => write!(f, "{e}"),
            CliError::Parse(e) => write!(f, "{e}"),
            CliError::ParseDiagnostic { error, .. } => write!(f, "{error}"),
            CliError::Validation(e) => write!(f, "{e}"),
            CliError::Usage(e) => write!(f, "{e}"),
        }
    }
}

impl From<io::Error> for CliError {
    fn from(e: io::Error) -> Self {
        CliError::Io(e)
    }
}

impl From<styx_tree::BuildError> for CliError {
    fn from(e: styx_tree::BuildError) -> Self {
        CliError::Parse(e.to_string())
    }
}

// ============================================================================
// File-first mode
// ============================================================================

#[derive(Default)]
struct FileOptions {
    input: Option<String>,
    output: Option<String>,
    json_out: Option<String>,
    in_place: bool,
    compact: bool,
    validate: bool,
    override_schema: Option<String>,
}

fn parse_file_options(args: &[String]) -> Result<FileOptions, CliError> {
    let mut opts = FileOptions::default();
    let mut i = 0;

    while i < args.len() {
        let arg = &args[i];

        if arg == "-o" {
            i += 1;
            opts.output = Some(
                args.get(i)
                    .ok_or_else(|| CliError::Usage("-o requires an argument".into()))?
                    .clone(),
            );
        } else if arg == "--json-out" {
            i += 1;
            opts.json_out = Some(
                args.get(i)
                    .ok_or_else(|| CliError::Usage("--json-out requires an argument".into()))?
                    .clone(),
            );
        } else if arg == "--in-place" {
            opts.in_place = true;
        } else if arg == "--compact" {
            opts.compact = true;
        } else if arg == "--validate" {
            opts.validate = true;
        } else if arg == "--override-schema" {
            i += 1;
            opts.override_schema = Some(
                args.get(i)
                    .ok_or_else(|| {
                        CliError::Usage("--override-schema requires an argument".into())
                    })?
                    .clone(),
            );
        } else if arg.starts_with('-') && arg != "-" {
            return Err(CliError::Usage(format!("unknown option: {arg}")));
        } else if opts.input.is_none() {
            opts.input = Some(arg.clone());
        } else {
            return Err(CliError::Usage(format!("unexpected argument: {arg}")));
        }

        i += 1;
    }

    // Validate option combinations
    if opts.in_place && opts.input.as_deref() == Some("-") {
        return Err(CliError::Usage(
            "--in-place cannot be used with stdin".into(),
        ));
    }

    if opts.in_place && opts.input.is_none() {
        return Err(CliError::Usage("--in-place requires an input file".into()));
    }

    if opts.override_schema.is_some() && !opts.validate {
        return Err(CliError::Usage(
            "--override-schema requires --validate".into(),
        ));
    }

    // Safety check: prevent -o pointing to same file as input
    if let (Some(input), Some(output)) = (&opts.input, &opts.output)
        && input != "-"
        && output != "-"
        && is_same_file(input, output)
    {
        return Err(CliError::Usage(
            "input and output are the same file\nhint: use --in-place to modify in place".into(),
        ));
    }

    Ok(opts)
}

fn is_same_file(a: &str, b: &str) -> bool {
    // Try to canonicalize both paths
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        // If either doesn't exist yet, compare the strings
        _ => a == b,
    }
}

fn run_file_first(args: &[String]) -> Result<(), CliError> {
    let opts = parse_file_options(args)?;

    // Read input
    let source = read_input(opts.input.as_deref())?;
    let filename = opts.input.as_deref().unwrap_or("<stdin>").to_string();

    // Parse
    let value = styx_tree::parse(&source).map_err(|e| CliError::ParseDiagnostic {
        error: e,
        source: source.clone(),
        filename: filename.clone(),
    })?;

    // Validate if requested
    if opts.validate {
        run_validation(
            &value,
            opts.input.as_deref(),
            opts.override_schema.as_deref(),
        )?;
    }

    // Determine output format and destination
    if let Some(json_path) = &opts.json_out {
        // JSON output
        let json = value_to_json(&value);
        let output =
            serde_json::to_string_pretty(&json).map_err(|e| CliError::Io(io::Error::other(e)))?;
        write_output(json_path, &output)?;
    } else {
        // Styx output
        let format_opts = if opts.compact {
            FormatOptions::default().inline()
        } else {
            FormatOptions::default()
        };
        let output = format_value(&value, format_opts);

        if opts.in_place {
            // Write to input file
            let path = opts.input.as_ref().unwrap();
            std::fs::write(path, &output)?;
        } else if let Some(out_path) = &opts.output {
            write_output(out_path, &output)?;
        } else {
            // Default: stdout
            print!("{output}");
        }
    }

    Ok(())
}

fn run_validation(
    value: &Value,
    input_path: Option<&str>,
    override_schema: Option<&str>,
) -> Result<(), CliError> {
    // Determine schema source
    let schema_file = if let Some(schema_path) = override_schema {
        // Use override schema
        load_schema_file(schema_path)?
    } else {
        // Look for @ key in document root for schema declaration
        let schema_ref = find_schema_declaration(value)?;
        match schema_ref {
            SchemaRef::External(path) => {
                // Resolve relative to input file's directory
                let resolved = resolve_schema_path(&path, input_path)?;
                load_schema_file(&resolved)?
            }
            SchemaRef::Inline(schema_value) => {
                // Parse inline schema
                parse_inline_schema(&schema_value)?
            }
        }
    };

    // Strip the @ key (schema declaration) from the value before validation
    let value_for_validation = strip_schema_declaration(value);

    // Run validation
    let result = validate(&value_for_validation, &schema_file);

    if !result.is_valid() {
        for error in &result.errors {
            eprintln!("{}", error);
        }
        return Err(CliError::Validation(format!(
            "{} validation error(s)",
            result.errors.len()
        )));
    }

    // Print warnings
    for warning in &result.warnings {
        eprintln!("warning: {}", warning.message);
    }

    Ok(())
}

enum SchemaRef {
    External(String),
    Inline(Value),
}

/// Strip the @ key (schema declaration) from a document before validation.
/// The @ key is metadata that references the schema, not actual data.
fn strip_schema_declaration(value: &Value) -> Value {
    if let Some(obj) = value.as_object() {
        let filtered_entries: Vec<_> = obj
            .entries
            .iter()
            .filter(|e| !e.key.is_unit())
            .cloned()
            .collect();
        Value {
            tag: value.tag.clone(),
            payload: Some(Payload::Object(styx_tree::Object {
                entries: filtered_entries,
                separator: obj.separator,
                span: obj.span,
            })),
            span: value.span,
        }
    } else {
        value.clone()
    }
}

fn find_schema_declaration(value: &Value) -> Result<SchemaRef, CliError> {
    // Look for @ key (unit key) in root object
    let obj = value.as_object().ok_or_else(|| {
        CliError::Validation("document root must be an object for validation".into())
    })?;

    for entry in &obj.entries {
        if entry.key.is_unit() {
            // Found @ key - check if it's a string (external) or object (inline)
            if let Some(path) = entry.value.as_str() {
                return Ok(SchemaRef::External(path.to_string()));
            } else if entry.value.as_object().is_some() {
                return Ok(SchemaRef::Inline(entry.value.clone()));
            } else {
                return Err(CliError::Validation(
                    "schema declaration (@) must be a path string or inline schema object".into(),
                ));
            }
        }
    }

    Err(CliError::Validation(
        "no schema declaration found (@ key)\nhint: use --override-schema to specify a schema file"
            .into(),
    ))
}

fn resolve_schema_path(schema_path: &str, input_path: Option<&str>) -> Result<String, CliError> {
    // If it's a URL, return as-is (not supported yet)
    if schema_path.starts_with("http://") || schema_path.starts_with("https://") {
        return Err(CliError::Usage(
            "URL schema references are not yet supported".into(),
        ));
    }

    // If absolute, return as-is
    let path = Path::new(schema_path);
    if path.is_absolute() {
        return Ok(schema_path.to_string());
    }

    // Resolve relative to input file's directory
    if let Some(input) = input_path
        && input != "-"
        && let Some(parent) = Path::new(input).parent()
    {
        return Ok(parent.join(schema_path).to_string_lossy().to_string());
    }

    // Fall back to current directory
    Ok(schema_path.to_string())
}

fn load_schema_file(path: &str) -> Result<SchemaFile, CliError> {
    let source = std::fs::read_to_string(path).map_err(|e| {
        CliError::Io(io::Error::new(
            e.kind(),
            format!("schema file '{}': {}", path, e),
        ))
    })?;

    facet_styx::from_str(&source)
        .map_err(|e| CliError::Parse(format!("failed to parse schema '{}': {}", path, e)))
}

fn parse_inline_schema(value: &Value) -> Result<SchemaFile, CliError> {
    // Inline schemas have simplified form - just the schema block is required
    // For now, serialize back to string and re-parse as SchemaFile
    // This is inefficient but correct - we can optimize later
    let source = styx_format::format_value(value, FormatOptions::default());
    facet_styx::from_str(&source)
        .map_err(|e| CliError::Parse(format!("failed to parse inline schema: {}", e)))
}

// ============================================================================
// Subcommand mode
// ============================================================================

fn run_subcommand(cmd: &str, args: &[String]) -> Result<(), CliError> {
    match cmd {
        "tree" => run_tree(args),
        "diff" => Err(CliError::Usage("@diff is not yet implemented".into())),
        "lsp" => run_lsp(args),
        _ => Err(CliError::Usage(format!("unknown subcommand: @{cmd}"))),
    }
}

fn run_lsp(_args: &[String]) -> Result<(), CliError> {
    let rt = tokio::runtime::Runtime::new().map_err(CliError::Io)?;
    rt.block_on(async {
        styx_lsp::run()
            .await
            .map_err(|e| CliError::Io(io::Error::other(e)))
    })
}

fn run_tree(args: &[String]) -> Result<(), CliError> {
    let file = args.first().map(|s| s.as_str());
    let source = read_input(file)?;
    let value = styx_tree::parse(&source)?;
    print_tree(&value, 0);
    Ok(())
}

// ============================================================================
// I/O helpers
// ============================================================================

fn read_input(file: Option<&str>) -> Result<String, io::Error> {
    match file {
        Some("-") | None => {
            let mut buf = String::new();
            io::stdin().read_to_string(&mut buf)?;
            Ok(buf)
        }
        Some(path) => std::fs::read_to_string(path),
    }
}

fn write_output(path: &str, content: &str) -> Result<(), io::Error> {
    if path == "-" {
        print!("{content}");
        Ok(())
    } else {
        std::fs::write(path, content)
    }
}

// ============================================================================
// Tree printing (debug)
// ============================================================================

fn print_tree(value: &Value, indent: usize) {
    let pad = "  ".repeat(indent);

    if let Some(tag) = &value.tag {
        print!("{pad}Tagged @{}", tag.name);
        match &value.payload {
            None => {
                println!();
            }
            Some(payload) => {
                println!(" {{");
                print_payload(payload, indent + 1);
                println!("{pad}}}");
            }
        }
    } else {
        match &value.payload {
            None => {
                println!("{pad}Unit");
            }
            Some(Payload::Scalar(s)) => {
                println!("{pad}Scalar({:?}, {:?})", s.text, s.kind);
            }
            Some(Payload::Sequence(s)) => {
                println!("{pad}Sequence [");
                for item in &s.items {
                    print_tree(item, indent + 1);
                }
                println!("{pad}]");
            }
            Some(Payload::Object(o)) => {
                println!("{pad}Object {{");
                for entry in &o.entries {
                    print!("{pad}  key: ");
                    print_tree_inline(&entry.key);
                    println!();
                    print!("{pad}  value: ");
                    if is_complex_value(&entry.value) {
                        println!();
                        print_tree(&entry.value, indent + 2);
                    } else {
                        print_tree_inline(&entry.value);
                        println!();
                    }
                }
                println!("{pad}}}");
            }
        }
    }
}

fn print_payload(payload: &Payload, indent: usize) {
    let pad = "  ".repeat(indent);
    match payload {
        Payload::Scalar(s) => {
            println!("{pad}Scalar({:?}, {:?})", s.text, s.kind);
        }
        Payload::Sequence(s) => {
            println!("{pad}Sequence [");
            for item in &s.items {
                print_tree(item, indent + 1);
            }
            println!("{pad}]");
        }
        Payload::Object(o) => {
            println!("{pad}Object {{");
            for entry in &o.entries {
                print!("{pad}  key: ");
                print_tree_inline(&entry.key);
                println!();
                print!("{pad}  value: ");
                if is_complex_value(&entry.value) {
                    println!();
                    print_tree(&entry.value, indent + 2);
                } else {
                    print_tree_inline(&entry.value);
                    println!();
                }
            }
            println!("{pad}}}");
        }
    }
}

fn is_complex_value(value: &Value) -> bool {
    if value.tag.is_some() && value.payload.is_some() {
        return true;
    }
    matches!(
        &value.payload,
        Some(Payload::Object(_)) | Some(Payload::Sequence(_))
    )
}

fn print_tree_inline(value: &Value) {
    if let Some(tag) = &value.tag {
        if value.payload.is_some() {
            print!("Tagged @{} {{...}}", tag.name);
        } else {
            print!("Tagged @{}", tag.name);
        }
    } else {
        match &value.payload {
            None => print!("Unit"),
            Some(Payload::Scalar(s)) => print!("Scalar({:?})", s.text),
            Some(Payload::Sequence(_)) => print!("Sequence [...]"),
            Some(Payload::Object(_)) => print!("Object {{...}}"),
        }
    }
}

// ============================================================================
// JSON conversion
// ============================================================================

fn value_to_json(value: &Value) -> serde_json::Value {
    if let Some(tag) = &value.tag {
        let mut obj = serde_json::Map::new();
        obj.insert(
            "$tag".to_string(),
            serde_json::Value::String(tag.name.clone()),
        );
        if let Some(payload) = &value.payload {
            obj.insert("$payload".to_string(), payload_to_json(payload));
        }
        serde_json::Value::Object(obj)
    } else {
        match &value.payload {
            None => serde_json::Value::Null,
            Some(payload) => payload_to_json(payload),
        }
    }
}

fn payload_to_json(payload: &Payload) -> serde_json::Value {
    match payload {
        Payload::Scalar(s) => serde_json::Value::String(s.text.clone()),
        Payload::Sequence(s) => {
            serde_json::Value::Array(s.items.iter().map(value_to_json).collect())
        }
        Payload::Object(o) => {
            let mut obj = serde_json::Map::new();
            for entry in &o.entries {
                let key = if entry.key.is_unit() {
                    "@".to_string()
                } else if let Some(s) = entry.key.as_str() {
                    s.to_string()
                } else {
                    format!("{:?}", entry.key)
                };
                obj.insert(key, value_to_json(&entry.value));
            }
            serde_json::Value::Object(obj)
        }
    }
}
