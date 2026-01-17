//! Styx CLI tool
//!
//! File-first design:
//!   styx <file> [options]         - operate on a file
//!   styx @<cmd> [args] [options]  - run a subcommand

use std::io::{self, Read};
use std::path::Path;

use styx_format::{FormatOptions, format_source};
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
    @tree [--format sexp|debug] <file>  Show parse tree (styx_tree)
    @cst <file>                     Show CST structure (styx_cst)
    @diff <old> <new>               Structural diff (not yet implemented)
    @lsp                            Start language server (stdio)
    @skill                          Output Claude Code skill for AI assistance

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
        run_validation(&value, &source, &filename, opts.override_schema.as_deref())?;
    }

    // Determine output format and destination
    if let Some(json_path) = &opts.json_out {
        // JSON output
        let json = value_to_json(&value);
        let output =
            serde_json::to_string_pretty(&json).map_err(|e| CliError::Io(io::Error::other(e)))?;
        write_output(json_path, &output)?;
    } else {
        // Styx output - use CST formatter to preserve comments
        let format_opts = if opts.compact {
            FormatOptions::default().inline()
        } else {
            FormatOptions::default()
        };
        let output = format_source(&source, format_opts);

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
    source: &str,
    filename: &str,
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
                let resolved = resolve_schema_path(&path, Some(filename))?;
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
        // Use ariadne for pretty error reporting
        result.write_report(filename, source, std::io::stderr());
        return Err(CliError::Validation(format!(
            "{} validation error(s)",
            result.errors.len()
        )));
    }

    // Print warnings (also with ariadne)
    if !result.warnings.is_empty() {
        result.write_report(filename, source, std::io::stderr());
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
        "cst" => run_cst(args),
        "diff" => Err(CliError::Usage("@diff is not yet implemented".into())),
        "lsp" => run_lsp(args),
        "skill" => run_skill(args),
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
    // Parse args: [--format sexp|debug] <file>
    let mut format = "debug";
    let mut file = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--format" {
            i += 1;
            format = args.get(i).map(|s| s.as_str()).ok_or_else(|| {
                CliError::Usage("--format requires an argument (sexp or debug)".into())
            })?;
        } else if args[i].starts_with('-') {
            return Err(CliError::Usage(format!("unknown option: {}", args[i])));
        } else if file.is_none() {
            file = Some(args[i].as_str());
        } else {
            return Err(CliError::Usage(format!("unexpected argument: {}", args[i])));
        }
        i += 1;
    }

    let source = read_input(file)?;
    let filename = file.unwrap_or("<stdin>");

    match format {
        "sexp" => {
            match styx_tree::parse(&source) {
                Ok(value) => {
                    println!("; file: {}", filename);
                    print_sexp(&value, 0);
                    println!();
                }
                Err(e) => {
                    // Output error in sexp format
                    let (start, end) = match &e {
                        styx_tree::BuildError::Parse(_, span) => (span.start, span.end),
                        _ => (0, 0),
                    };
                    let msg = json_escape(&e.to_string());
                    println!("; file: {}", filename);
                    println!("(error [{}, {}] \"{}\")", start, end, msg);
                }
            }
        }
        "debug" => {
            let value = styx_tree::parse(&source)?;
            print_tree(&value, 0);
        }
        _ => {
            return Err(CliError::Usage(format!(
                "unknown format '{}', expected 'sexp' or 'debug'",
                format
            )));
        }
    }

    Ok(())
}

fn run_skill(_args: &[String]) -> Result<(), CliError> {
    print!("{}", include_str!("../../../contrib/claude-skill/SKILL.md"));
    Ok(())
}

fn run_cst(args: &[String]) -> Result<(), CliError> {
    let file = args.first().map(|s| s.as_str());
    let source = read_input(file)?;

    let parsed = styx_cst::parse(&source);

    // Print the CST using Debug format
    println!("{:#?}", parsed.syntax());

    // Print parse errors if any
    if !parsed.errors().is_empty() {
        println!("\nParse errors:");
        for err in parsed.errors() {
            println!("  {:?}", err);
        }
    }

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
// S-expression output (compliance format)
// ============================================================================

use styx_parse::ScalarKind;

fn print_sexp(value: &Value, indent: usize) {
    // The root value is always an object representing the document
    let pad = "  ".repeat(indent);

    if let Some(obj) = value.as_object() {
        let span = value
            .span
            .map(|s| format!("[{}, {}]", s.start, s.end))
            .unwrap_or_else(|| "[-1, -1]".to_string());
        println!("{pad}(document {span}");
        for entry in &obj.entries {
            print_sexp_entry(entry, indent + 1);
        }
        print!("{pad})");
    } else {
        // Shouldn't happen for a parsed document, but handle it
        print_sexp_value(value, indent);
    }
}

fn print_sexp_entry(entry: &styx_tree::Entry, indent: usize) {
    let pad = "  ".repeat(indent);
    println!("{pad}(entry");
    print_sexp_value(&entry.key, indent + 1);
    println!();
    print_sexp_value(&entry.value, indent + 1);
    print!(")");
    println!();
}

fn print_sexp_value(value: &Value, indent: usize) {
    let pad = "  ".repeat(indent);
    let span = value
        .span
        .map(|s| format!("[{}, {}]", s.start, s.end))
        .unwrap_or_else(|| "[-1, -1]".to_string());

    match (&value.tag, &value.payload) {
        // Unit: no tag, no payload
        (None, None) => {
            print!("{pad}(unit {span})");
        }
        // Tagged value (with or without payload)
        (Some(tag), payload) => {
            let tag_name = json_escape(&tag.name);
            print!("{pad}(tag {span} \"{tag_name}\"");
            if let Some(p) = payload {
                println!();
                print_sexp_payload(p, indent + 1);
                print!(")");
            } else {
                print!(")");
            }
        }
        // Untagged scalar
        (None, Some(Payload::Scalar(s))) => {
            let kind = match s.kind {
                ScalarKind::Bare => "bare",
                ScalarKind::Quoted => "quoted",
                ScalarKind::Raw => "raw",
                ScalarKind::Heredoc => "heredoc",
            };
            let text = json_escape(&s.text);
            print!("{pad}(scalar {span} {kind} \"{text}\")");
        }
        // Untagged sequence
        (None, Some(Payload::Sequence(seq))) => {
            print!("{pad}(sequence {span}");
            if seq.items.is_empty() {
                print!(")");
            } else {
                println!();
                for (i, item) in seq.items.iter().enumerate() {
                    print_sexp_value(item, indent + 1);
                    if i < seq.items.len() - 1 {
                        println!();
                    }
                }
                print!(")");
            }
        }
        // Untagged object
        (None, Some(Payload::Object(obj))) => {
            let sep = match obj.separator {
                styx_parse::Separator::Newline => "newline",
                styx_parse::Separator::Comma => "comma",
            };
            print!("{pad}(object {span} {sep}");
            if obj.entries.is_empty() {
                print!(")");
            } else {
                println!();
                for entry in &obj.entries {
                    print_sexp_entry(entry, indent + 1);
                }
                print!("{pad})");
            }
        }
    }
}

fn print_sexp_payload(payload: &Payload, indent: usize) {
    let pad = "  ".repeat(indent);
    match payload {
        Payload::Scalar(s) => {
            let span = s
                .span
                .map(|sp| format!("[{}, {}]", sp.start, sp.end))
                .unwrap_or_else(|| "[-1, -1]".to_string());
            let kind = match s.kind {
                ScalarKind::Bare => "bare",
                ScalarKind::Quoted => "quoted",
                ScalarKind::Raw => "raw",
                ScalarKind::Heredoc => "heredoc",
            };
            let text = json_escape(&s.text);
            print!("{pad}(scalar {span} {kind} \"{text}\")");
        }
        Payload::Sequence(seq) => {
            let span = seq
                .span
                .map(|s| format!("[{}, {}]", s.start, s.end))
                .unwrap_or_else(|| "[-1, -1]".to_string());
            print!("{pad}(sequence {span}");
            if seq.items.is_empty() {
                print!(")");
            } else {
                println!();
                for (i, item) in seq.items.iter().enumerate() {
                    print_sexp_value(item, indent + 1);
                    if i < seq.items.len() - 1 {
                        println!();
                    }
                }
                print!(")");
            }
        }
        Payload::Object(obj) => {
            let span = obj
                .span
                .map(|s| format!("[{}, {}]", s.start, s.end))
                .unwrap_or_else(|| "[-1, -1]".to_string());
            let sep = match obj.separator {
                styx_parse::Separator::Newline => "newline",
                styx_parse::Separator::Comma => "comma",
            };
            print!("{pad}(object {span} {sep}");
            if obj.entries.is_empty() {
                print!(")");
            } else {
                println!();
                for entry in &obj.entries {
                    print_sexp_entry(entry, indent + 1);
                }
                print!("{pad})");
            }
        }
    }
}

fn json_escape(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c if c.is_control() => {
                result.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => result.push(c),
        }
    }
    result
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
