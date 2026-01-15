//! Styx CLI tool
//!
//! File-first design:
//!   styx <file> [options]         - operate on a file
//!   styx @<cmd> [args] [options]  - run a subcommand

use std::io::{self, Read};

use styx_format::{FormatOptions, format_value};
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

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let result = if args.is_empty() {
        print_usage();
        Ok(())
    } else if args[0] == "--help" || args[0] == "-h" {
        print_usage();
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
            eprintln!("error: {e}");
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
    @lsp                            Start language server (not yet implemented)

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
    Validation(String),
    Usage(String),
}

impl CliError {
    fn exit_code(&self) -> i32 {
        match self {
            CliError::Io(_) => EXIT_IO_ERROR,
            CliError::Parse(_) => EXIT_SYNTAX_ERROR,
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
    if let (Some(input), Some(output)) = (&opts.input, &opts.output) {
        if input != "-" && output != "-" && is_same_file(input, output) {
            return Err(CliError::Usage(
                "input and output are the same file\nhint: use --in-place to modify in place"
                    .into(),
            ));
        }
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

    // Parse
    let value = styx_tree::parse(&source)?;

    // Validate if requested
    if opts.validate {
        run_validation(&value, &source, opts.override_schema.as_deref())?;
    }

    // Determine output format and destination
    if let Some(json_path) = &opts.json_out {
        // JSON output
        let json = value_to_json(&value);
        let output = serde_json::to_string_pretty(&json)
            .map_err(|e| CliError::Io(io::Error::new(io::ErrorKind::Other, e)))?;
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
    _value: &Value,
    _source: &str,
    _override_schema: Option<&str>,
) -> Result<(), CliError> {
    // TODO: Implement validation
    // 1. Look for @ key in document root for schema declaration
    // 2. Or use override_schema if provided
    // 3. Load and parse schema
    // 4. Run styx_schema::validate
    Err(CliError::Usage("--validate is not yet implemented".into()))
}

// ============================================================================
// Subcommand mode
// ============================================================================

fn run_subcommand(cmd: &str, args: &[String]) -> Result<(), CliError> {
    match cmd {
        "tree" => run_tree(args),
        "diff" => Err(CliError::Usage("@diff is not yet implemented".into())),
        "lsp" => Err(CliError::Usage("@lsp is not yet implemented".into())),
        _ => Err(CliError::Usage(format!("unknown subcommand: @{cmd}"))),
    }
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
