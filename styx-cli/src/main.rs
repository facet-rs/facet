#![doc = include_str!("../README.md")]
//! Styx CLI tool
//!
//! Disambiguation heuristic:
//!   If arg contains '.' or '/' → file mode
//!   If arg is '-' → stdin (file mode)
//!   Otherwise → subcommand mode
//!
//! Examples:
//!   styx config.styx              - file mode (has '.')
//!   styx ./config                 - file mode (has '/')
//!   styx -                        - stdin
//!   styx lsp                      - subcommand (bare word)
//!   styx tree config.styx         - subcommand with file arg

use std::io::{self, IsTerminal, Read};
use std::path::Path;

use facet::Facet;
use facet_styx::{SchemaFile, validate};
use figue as args;
use styx_format::{FormatOptions, format_source};
use styx_lsp::{TokenType, compute_highlight_spans};
use styx_tree::{Payload, Value};

// ============================================================================
// Exit codes
// ============================================================================

const EXIT_SUCCESS: i32 = 0;
const EXIT_SYNTAX_ERROR: i32 = 1;
const EXIT_VALIDATION_ERROR: i32 = 2;
const EXIT_IO_ERROR: i32 = 3;

// ============================================================================
// CLI argument structures
// ============================================================================

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// File mode arguments: `styx <file> [options]`
#[derive(Facet, Debug, Default)]
struct FileArgs {
    /// Input file path (or "-" for stdin)
    #[facet(args::positional)]
    input: String,

    /// Output to file (styx format)
    #[facet(args::named, args::short = 'o', default)]
    output: Option<String>,

    /// Output as JSON to file (or "-" for stdout)
    #[facet(args::named, default)]
    json_out: Option<String>,

    /// Modify input file in place
    #[facet(args::named, default)]
    in_place: bool,

    /// Single-line/compact formatting
    #[facet(args::named, default)]
    compact: bool,

    /// Validate against declared schema (no output unless -o specified)
    #[facet(args::named, default)]
    validate: bool,

    /// Use this schema instead of declared @schema
    #[facet(args::named, default)]
    schema: Option<String>,
}

/// Top-level CLI with optional subcommand
#[derive(Facet, Debug)]
struct Args {
    /// Show version
    #[facet(args::named, args::short = 'V', default)]
    version: bool,

    /// Subcommand to run
    #[facet(args::subcommand, default)]
    command: Option<Command>,
}

/// Available subcommands
#[derive(Facet, Debug)]
#[repr(u8)]
enum Command {
    /// Start language server (stdio)
    Lsp,

    /// Show parse tree
    Tree {
        /// Output format: sexp or debug
        #[facet(args::named, default = "debug")]
        format: String,

        /// Input file
        #[facet(args::positional)]
        file: String,
    },

    /// Show CST structure
    Cst {
        /// Input file
        #[facet(args::positional)]
        file: String,
    },

    /// Extract embedded schemas from a binary
    Extract {
        /// Binary file to extract from
        #[facet(args::positional)]
        binary: String,
    },

    /// Compare schema against published version
    Diff {
        /// Schema file to compare
        #[facet(args::positional)]
        schema: String,

        /// Crate name on staging.crates.io
        #[facet(args::named, rename = "crate")]
        crate_name: String,

        /// Baseline version (default: latest)
        #[facet(args::named, default)]
        baseline: Option<String>,
    },

    /// Generate publishable crate from schema
    Package {
        /// Schema file
        #[facet(args::positional)]
        schema: String,

        /// Crate name
        #[facet(args::named)]
        name: String,

        /// Crate version
        #[facet(args::named)]
        version: String,

        /// Output directory (default: `./<name>`)
        #[facet(args::named, default)]
        output: Option<String>,
    },

    /// Publish schema to staging.crates.io
    Publish {
        /// Schema file
        #[facet(args::positional)]
        schema: String,

        /// Skip confirmation prompt
        #[facet(args::named, args::short = 'y', default)]
        yes: bool,
    },

    /// Cache management
    Cache {
        /// Open cache directory in file explorer
        #[facet(args::named, default)]
        open: bool,

        /// Clear all cached schemas
        #[facet(args::named, default)]
        clear: bool,
    },

    /// Output Claude Code skill for AI assistance
    Skill,

    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        #[facet(args::positional)]
        shell: String,
    },

    /// Generate code from schema
    Gen {
        /// Target language
        #[facet(args::positional)]
        language: String,

        /// Schema file
        #[facet(args::positional)]
        schema: String,

        /// Output directory (default: current directory)
        #[facet(args::named, default)]
        output: Option<String>,

        /// Package name (for Go: defaults to schema basename)
        #[facet(args::named, default)]
        package: Option<String>,
    },
}

// ============================================================================
// Main entry point
// ============================================================================

/// Determines if an argument should be treated as a file path.
///
/// Returns true if the argument:
/// - Contains '.' (e.g., config.styx, file.json)
/// - Contains '/' (e.g., ./config, ../path, /absolute/path)
/// - Is exactly '-' (stdin)
fn is_file_arg(arg: &str) -> bool {
    arg == "-" || arg.contains('.') || arg.contains('/')
}

fn main() {
    let raw_args: Vec<String> = std::env::args().skip(1).collect();

    // Handle empty args or help
    if raw_args.is_empty() {
        print_help();
        std::process::exit(EXIT_SUCCESS);
    }

    // Handle --version / -V at top level
    if raw_args[0] == "--version" || raw_args[0] == "-V" {
        println!("styx {VERSION}");
        std::process::exit(EXIT_SUCCESS);
    }

    // Handle --help / -h at top level
    if raw_args[0] == "--help" || raw_args[0] == "-h" {
        print_help();
        std::process::exit(EXIT_SUCCESS);
    }

    // Disambiguation: is first arg a file or a subcommand?
    let result = if is_file_arg(&raw_args[0]) {
        // File mode: parse as FileArgs
        run_file_mode(&raw_args)
    } else {
        // Subcommand mode: parse as Args with subcommand
        run_subcommand_mode(&raw_args)
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

fn print_help() {
    eprintln!("styx {VERSION} - command-line tool for Styx configuration files\n");
    eprintln!("USAGE:");
    eprintln!("    styx <file> [options]           Process a Styx file");
    eprintln!("    styx <command> [args]           Run a subcommand\n");
    eprintln!("    Files are detected by '.' or '/' in the name, or '-' for stdin.");
    eprintln!("    Bare words (e.g., 'lsp', 'tree') are subcommands.\n");
    eprintln!("FILE MODE OPTIONS:");
    eprintln!("    -o, --output <FILE>             Output to file (styx format)");
    eprintln!("        --json-out <FILE>           Output as JSON (use '-' for stdout)");
    eprintln!("        --in-place                  Modify input file in place");
    eprintln!("        --compact                   Single-line/compact formatting");
    eprintln!("        --validate                  Validate against declared schema");
    eprintln!("        --schema <FILE>             Use this schema instead of @schema\n");
    eprintln!("SUBCOMMANDS:");
    eprintln!("    lsp                             Start language server (stdio)");
    eprintln!("    tree <file>                     Show parse tree");
    eprintln!("    cst <file>                      Show CST structure");
    eprintln!("    extract <binary>                Extract embedded schemas");
    eprintln!("    diff <schema> --crate <name>    Compare against published version");
    eprintln!("    package <schema> --name <n> --version <v>");
    eprintln!("                                    Generate publishable crate");
    eprintln!("    publish <schema> [-y]           Publish to staging.crates.io");
    eprintln!("    cache [--open|--clear]          Cache management");
    eprintln!("    skill                           Output Claude Code skill");
    eprintln!("    completions <shell>             Generate shell completions (bash, zsh, fish)");
    eprintln!("    gen <lang> <schema>             Generate code from schema (go)\n");
    eprintln!("EXAMPLES:");
    eprintln!("    styx config.styx                Format and print to stdout");
    eprintln!("    styx config.styx --in-place     Format file in place");
    eprintln!("    styx config.styx --validate     Validate against schema");
    eprintln!("    styx tree config.styx           Show parse tree");
    eprintln!("    styx completions bash           Generate bash completions");
}

fn run_file_mode(args: &[String]) -> Result<(), CliError> {
    let args_strs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let opts: FileArgs = figue::from_slice(&args_strs).unwrap();

    // Validate option combinations
    if opts.in_place && opts.input == "-" {
        return Err(CliError::Usage(
            "--in-place cannot be used with stdin".into(),
        ));
    }

    if opts.schema.is_some() && !opts.validate {
        return Err(CliError::Usage("--schema requires --validate".into()));
    }

    // Safety check: prevent -o pointing to same file as input
    if let Some(ref output) = opts.output
        && opts.input != "-"
        && output != "-"
        && is_same_file(&opts.input, output)
    {
        return Err(CliError::Usage(
            "input and output are the same file\nhint: use --in-place to modify in place".into(),
        ));
    }

    // Read input
    let source = read_input(Some(&opts.input))?;
    let filename = if opts.input == "-" {
        "<stdin>".to_string()
    } else {
        opts.input.clone()
    };

    // Parse
    let value = styx_tree::parse(&source).map_err(|e| CliError::ParseDiagnostic {
        error: e,
        source: source.clone(),
        filename: filename.clone(),
    })?;

    // Validate if requested
    if opts.validate {
        run_validation(&value, &source, &filename, opts.schema.as_deref())?;
    }

    // If --validate with no explicit output, we're done (exit code only)
    let has_explicit_output = opts.json_out.is_some() || opts.output.is_some() || opts.in_place;
    if opts.validate && !has_explicit_output {
        return Ok(());
    }

    // Determine output format and destination
    if let Some(ref json_path) = opts.json_out {
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
            std::fs::write(&opts.input, &output)?;
        } else if let Some(ref out_path) = opts.output {
            write_output(out_path, &output)?;
        } else {
            print_styx(&output);
        }
    }

    Ok(())
}

fn run_subcommand_mode(args: &[String]) -> Result<(), CliError> {
    let args_strs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let parsed: Args = figue::from_slice(&args_strs).unwrap();

    match parsed.command {
        Some(Command::Lsp) => run_lsp(),
        Some(Command::Tree { format, file }) => run_tree(&format, &file),
        Some(Command::Cst { file }) => run_cst(&file),
        Some(Command::Extract { binary }) => run_extract(&binary),
        Some(Command::Diff {
            schema,
            crate_name,
            baseline,
        }) => run_diff(&schema, &crate_name, baseline.as_deref()),
        Some(Command::Package {
            schema,
            name,
            version,
            output,
        }) => run_package(&schema, &name, &version, output.as_deref()),
        Some(Command::Publish { schema, yes }) => run_publish(&schema, yes),
        Some(Command::Cache { open, clear }) => run_cache(open, clear),
        Some(Command::Skill) => run_skill(),
        Some(Command::Completions { shell }) => run_completions(&shell),
        Some(Command::Gen {
            language,
            schema,
            output,
            package,
        }) => run_gen(&language, &schema, output.as_deref(), package.as_deref()),
        None => {
            print_help();
            Ok(())
        }
    }
}

// ============================================================================
// Error handling
// ============================================================================

#[derive(Debug)]
#[allow(dead_code)]
enum CliError {
    Io(io::Error),
    Parse(String),
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

impl From<styx_gen_go::GenError> for CliError {
    fn from(e: styx_gen_go::GenError) -> Self {
        CliError::Io(io::Error::other(e.to_string()))
    }
}

// ============================================================================
// Subcommand implementations
// ============================================================================

fn run_lsp() -> Result<(), CliError> {
    let rt = tokio::runtime::Runtime::new().map_err(CliError::Io)?;
    rt.block_on(async {
        styx_lsp::run()
            .await
            .map_err(|e| CliError::Io(io::Error::other(e)))
    })
}

fn run_tree(format: &str, file: &str) -> Result<(), CliError> {
    let source = read_input(Some(file))?;
    let filename = if file == "-" { "<stdin>" } else { file };

    match format {
        "sexp" => match styx_tree::parse(&source) {
            Ok(value) => {
                println!("; file: {}", filename);
                print_sexp(&value, 0);
                println!();
            }
            Err(e) => {
                let (start, end) = match &e {
                    styx_tree::BuildError::Parse(_, span) => (span.start, span.end),
                    _ => (0, 0),
                };
                let msg = json_escape(&e.to_string());
                println!("; file: {}", filename);
                println!("(error [{}, {}] \"{}\")", start, end, msg);
            }
        },
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

fn run_cst(file: &str) -> Result<(), CliError> {
    let source = read_input(Some(file))?;
    let parsed = styx_cst::parse(&source);

    println!("{:#?}", parsed.syntax());

    if !parsed.errors().is_empty() {
        println!("\nParse errors:");
        for err in parsed.errors() {
            println!("  {:?}", err);
        }
    }

    Ok(())
}

fn run_extract(binary: &str) -> Result<(), CliError> {
    let schemas = styx_embed::extract_schemas_from_file(Path::new(binary))
        .map_err(|e| CliError::Io(io::Error::other(format!("{binary}: {e}"))))?;

    if schemas.is_empty() {
        return Err(CliError::Usage(format!(
            "no embedded schemas found in {binary}"
        )));
    }

    for (i, schema) in schemas.iter().enumerate() {
        if schemas.len() > 1 {
            eprintln!("--- schema {} ---", i + 1);
        }
        print_styx(schema);
        // Ensure newline after schema (print_styx doesn't add one)
        if !schema.ends_with('\n') {
            println!();
        }
    }

    Ok(())
}

fn run_skill() -> Result<(), CliError> {
    print!("{}", include_str!("../contrib/SKILL.md"));
    Ok(())
}

fn run_completions(shell: &str) -> Result<(), CliError> {
    let shell_enum = match shell.to_lowercase().as_str() {
        "bash" => figue::Shell::Bash,
        "zsh" => figue::Shell::Zsh,
        "fish" => figue::Shell::Fish,
        _ => {
            return Err(CliError::Usage(format!(
                "unknown shell '{}', expected: bash, zsh, fish",
                shell
            )));
        }
    };

    let completions = figue::generate_completions::<Args>(shell_enum, "styx");
    print!("{completions}");
    Ok(())
}

fn run_gen(
    language: &str,
    schema_file: &str,
    output: Option<&str>,
    package: Option<&str>,
) -> Result<(), CliError> {
    match language.to_lowercase().as_str() {
        "go" => {
            // Load and parse schema
            let schema_content = std::fs::read_to_string(schema_file).map_err(|e| {
                CliError::Io(io::Error::new(
                    e.kind(),
                    format!("schema file '{}': {}", schema_file, e),
                ))
            })?;

            let schema: facet_styx::SchemaFile = facet_styx::from_str(&schema_content)
                .map_err(|e| CliError::Parse(format!("failed to parse schema: {}", e)))?;

            // Determine package name
            let pkg_name = package.unwrap_or_else(|| {
                Path::new(schema_file)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("config")
            });

            // Sanitize package name for Go (replace hyphens with underscores)
            let sanitized_pkg_name = pkg_name.replace('-', "_");

            // Determine output directory
            let output_dir = output.unwrap_or(".");

            // Generate Go code
            styx_gen_go::generate(&schema, &sanitized_pkg_name, output_dir)?;

            eprintln!("Generated Go code in {}/", output_dir);
            Ok(())
        }
        _ => Err(CliError::Usage(format!(
            "unknown language '{}', expected: go",
            language
        ))),
    }
}

fn run_cache(open: bool, clear: bool) -> Result<(), CliError> {
    use styx_lsp::cache;

    if clear {
        match cache::clear_cache() {
            Ok((count, size)) => {
                println!("Cleared {} cached schemas ({} bytes)", count, size);
            }
            Err(e) => {
                return Err(CliError::Io(e));
            }
        }
        return Ok(());
    }

    let Some(cache_dir) = cache::cache_dir() else {
        return Err(CliError::Usage(
            "could not determine cache directory".into(),
        ));
    };

    if open {
        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("open")
                .arg(&cache_dir)
                .spawn()
                .map_err(CliError::Io)?;
        }
        #[cfg(target_os = "linux")]
        {
            std::process::Command::new("xdg-open")
                .arg(&cache_dir)
                .spawn()
                .map_err(CliError::Io)?;
        }
        #[cfg(target_os = "windows")]
        {
            std::process::Command::new("explorer")
                .arg(&cache_dir)
                .spawn()
                .map_err(CliError::Io)?;
        }
        return Ok(());
    }

    println!("Cache directory: {}", cache_dir.display());

    if let Some(stats) = cache::cache_stats() {
        println!(
            "Embedded schemas: {} ({} bytes)",
            stats.embedded_count, stats.embedded_size
        );
        println!(
            "Crate schemas: {} ({} bytes)",
            stats.crate_count, stats.crate_size
        );
    } else {
        println!("(cache directory does not exist)");
    }

    Ok(())
}

// ============================================================================
// Validation
// ============================================================================

fn run_validation(
    value: &Value,
    source: &str,
    filename: &str,
    override_schema: Option<&str>,
) -> Result<(), CliError> {
    let schema_file = if let Some(schema_path) = override_schema {
        load_schema_file(schema_path)?
    } else {
        let schema_ref = find_schema_declaration(value)?;
        match schema_ref {
            SchemaRef::External(path) => {
                let resolved = resolve_schema_path(&path, Some(filename))?;
                load_schema_file(&resolved)?
            }
            SchemaRef::Embedded { id, cli } => extract_embedded_schema(&cli, id.as_deref())?,
        }
    };

    let value_for_validation = strip_schema_declaration(value);
    let result = validate(&value_for_validation, &schema_file);

    if !result.is_valid() {
        result.write_report(filename, source, std::io::stderr());
        return Err(CliError::Validation(format!(
            "{} validation error(s)",
            result.errors.len()
        )));
    }

    if !result.warnings.is_empty() {
        result.write_report(filename, source, std::io::stderr());
    }

    Ok(())
}

enum SchemaRef {
    External(String),
    Embedded { id: Option<String>, cli: String },
}

fn strip_schema_declaration(value: &Value) -> Value {
    if let Some(obj) = value.as_object() {
        let filtered_entries: Vec<_> = obj
            .entries
            .iter()
            .filter(|e| !e.key.is_schema_tag())
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
    let obj = value.as_object().ok_or_else(|| {
        CliError::Validation("document root must be an object for validation".into())
    })?;

    for entry in &obj.entries {
        if entry.key.is_schema_tag() {
            if let Some(path) = entry.value.as_str() {
                return Ok(SchemaRef::External(path.to_string()));
            }

            if let Some(schema_obj) = entry.value.as_object() {
                if let Some(cli_value) = schema_obj.get("cli")
                    && let Some(cli_name) = cli_value.as_str()
                {
                    // Also extract the optional schema ID
                    let id = schema_obj
                        .get("id")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    return Ok(SchemaRef::Embedded {
                        id,
                        cli: cli_name.to_string(),
                    });
                }
                return Err(CliError::Validation(
                    "@schema directive must have a 'cli' field with the binary name".into(),
                ));
            }

            return Err(CliError::Validation(
                "@schema directive must be a path or an object with {id ..., cli ...}".into(),
            ));
        }
    }

    Err(CliError::Validation(
        "no schema declaration found (@schema key)\nhint: use --schema to specify a schema file"
            .into(),
    ))
}

fn resolve_schema_path(schema_path: &str, input_path: Option<&str>) -> Result<String, CliError> {
    if schema_path.starts_with("http://") || schema_path.starts_with("https://") {
        return Err(CliError::Usage(
            "URL schema references are not yet supported".into(),
        ));
    }

    let path = Path::new(schema_path);
    if path.is_absolute() {
        return Ok(schema_path.to_string());
    }

    if let Some(input) = input_path
        && input != "-"
        && let Some(parent) = Path::new(input).parent()
    {
        return Ok(parent.join(schema_path).to_string_lossy().to_string());
    }

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

fn extract_embedded_schema(
    cli_name: &str,
    schema_id: Option<&str>,
) -> Result<SchemaFile, CliError> {
    let binary_path = which::which(cli_name).map_err(|_| {
        CliError::Validation(format!(
            "binary '{}' not found in PATH\nhint: ensure the binary is installed and in your PATH",
            cli_name
        ))
    })?;

    let schemas = styx_embed::extract_schemas_from_file(&binary_path).map_err(|e| {
        CliError::Validation(format!(
            "failed to extract schema from '{}': {}\nhint: the binary may not have embedded schemas",
            binary_path.display(),
            e
        ))
    })?;

    if schemas.is_empty() {
        return Err(CliError::Validation(format!(
            "no embedded schemas found in '{}'",
            binary_path.display()
        )));
    }

    // If a schema ID is specified, find the matching schema
    let schema_source = if let Some(target_id) = schema_id {
        schemas
            .iter()
            .find(|schema| {
                // Parse the schema to check its meta.id
                if let Ok(parsed) = styx_tree::parse(schema)
                    && let Some(obj) = parsed.as_object()
                    && let Some(meta) = obj.get("meta")
                    && let Some(meta_obj) = meta.as_object()
                    && let Some(id_value) = meta_obj.get("id")
                    && let Some(id) = id_value.as_str()
                {
                    return id == target_id;
                }
                false
            })
            .ok_or_else(|| {
                let available_ids: Vec<_> = schemas
                    .iter()
                    .filter_map(|schema| {
                        styx_tree::parse(schema).ok().and_then(|parsed| {
                            parsed.as_object().and_then(|obj| {
                                obj.get("meta").and_then(|meta| {
                                    meta.as_object()
                                        .and_then(|m| m.get("id").and_then(|v| v.as_str()))
                                        .map(|s| s.to_string())
                                })
                            })
                        })
                    })
                    .collect();
                CliError::Validation(format!(
                    "schema '{}' not found in '{}'\navailable schemas: {}",
                    target_id,
                    binary_path.display(),
                    available_ids.join(", ")
                ))
            })?
    } else {
        // No ID specified, use the first schema
        &schemas[0]
    };

    facet_styx::from_str(schema_source).map_err(|e| {
        CliError::Parse(format!(
            "failed to parse embedded schema from '{}': {}",
            binary_path.display(),
            e
        ))
    })
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
// Semantic highlighting for terminal output
// ============================================================================

/// ANSI color codes for different token types
mod ansi {
    pub const RESET: &str = "\x1b[0m";
    pub const COMMENT: &str = "\x1b[38;5;243m"; // Gray
    pub const DOC_COMMENT: &str = "\x1b[38;5;71m"; // Green (like doc strings)
    pub const STRING: &str = "\x1b[38;5;214m"; // Orange
    pub const NUMBER: &str = "\x1b[38;5;141m"; // Purple
    pub const KEYWORD: &str = "\x1b[38;5;203m"; // Red
    pub const TYPE: &str = "\x1b[38;5;80m"; // Cyan
    pub const ENUM_MEMBER: &str = "\x1b[38;5;80m"; // Cyan
    pub const PROPERTY: &str = "\x1b[38;5;75m"; // Blue
    pub const OPERATOR: &str = "\x1b[38;5;203m"; // Red
}

/// Get ANSI color code for a token type
fn ansi_color_for_token(token_type: TokenType, is_doc_comment: bool) -> &'static str {
    if is_doc_comment {
        return ansi::DOC_COMMENT;
    }
    match token_type {
        TokenType::Comment => ansi::COMMENT,
        TokenType::String => ansi::STRING,
        TokenType::Number => ansi::NUMBER,
        TokenType::Keyword => ansi::KEYWORD,
        TokenType::Type => ansi::TYPE,
        TokenType::EnumMember => ansi::ENUM_MEMBER,
        TokenType::Property => ansi::PROPERTY,
        TokenType::Operator => ansi::OPERATOR,
    }
}

/// Apply semantic highlighting to Styx source using ANSI escape codes
fn highlight_styx(source: &str) -> String {
    let parse = styx_cst::parse(source);
    let spans = compute_highlight_spans(&parse);

    if spans.is_empty() {
        return source.to_string();
    }

    let mut result = String::with_capacity(source.len() * 2);
    let mut last_end = 0;

    for span in &spans {
        // Add unhighlighted text before this span
        if span.start > last_end {
            result.push_str(&source[last_end..span.start]);
        }

        // Add highlighted span
        let color = ansi_color_for_token(span.token_type, span.is_doc_comment);
        result.push_str(color);
        result.push_str(&source[span.start..span.end]);
        result.push_str(ansi::RESET);

        last_end = span.end;
    }

    // Add remaining unhighlighted text
    if last_end < source.len() {
        result.push_str(&source[last_end..]);
    }

    result
}

/// Print Styx source with highlighting if stdout is a TTY
fn print_styx(source: &str) {
    if std::io::stdout().is_terminal() {
        print!("{}", highlight_styx(source));
    } else {
        print!("{source}");
    }
}

fn is_same_file(a: &str, b: &str) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
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
        (None, None) => {
            print!("{pad}(unit {span})");
        }
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

// ============================================================================
// Package command
// ============================================================================

fn run_package(
    schema_file: &str,
    name: &str,
    version: &str,
    output: Option<&str>,
) -> Result<(), CliError> {
    let output_dir = output.unwrap_or(name);
    let output_path = Path::new(output_dir);

    let schema_content = std::fs::read_to_string(schema_file)
        .map_err(|e| CliError::Io(io::Error::new(e.kind(), format!("{schema_file}: {e}"))))?;

    styx_tree::parse(&schema_content)
        .map_err(|e| CliError::Parse(format!("invalid schema: {e}")))?;

    std::fs::create_dir_all(output_path.join("src"))?;

    let cargo_toml = generate_cargo_toml(name, version);
    std::fs::write(output_path.join("Cargo.toml"), cargo_toml)?;

    let lib_rs = generate_lib_rs(name);
    std::fs::write(output_path.join("src/lib.rs"), lib_rs)?;

    let readme = generate_readme(name);
    std::fs::write(output_path.join("README.md"), readme)?;

    std::fs::write(output_path.join("schema.styx"), &schema_content)?;

    eprintln!("Created crate in {output_dir}/");
    eprintln!();
    eprintln!("To publish:");
    eprintln!("  cd {output_dir} && cargo publish");

    Ok(())
}

fn generate_cargo_toml(name: &str, version: &str) -> String {
    format!(
        r#"[package]
name = "{name}"
version = "{version}"
edition = "2024"
license = "MIT OR Apache-2.0"
description = "Styx schema for {name}"
readme = "README.md"
categories = ["config"]
keywords = ["styx", "schema"]
"#
    )
}

fn generate_readme(name: &str) -> String {
    let crate_name_snake = name.replace('-', "_");
    format!(
        r#"# {name}

A [Styx](https://styx.bearcove.eu) schema crate.

## Usage

```rust
use {crate_name_snake}::SCHEMA;
```

## License

MIT OR Apache-2.0
"#
    )
}

fn generate_lib_rs(name: &str) -> String {
    let crate_name_snake = name.replace('-', "_");
    format!(
        r#"//! Styx schema crate for {name}.

/// The styx schema content.
pub const SCHEMA: &str = include_str!("../schema.styx");
"#,
        name = crate_name_snake
    )
}

// ============================================================================
// Publish command
// ============================================================================

const STAGING_INDEX: &str = "sparse+https://index.staging.crates.io/";
const STAGING_API: &str = "https://staging.crates.io/api/v1/crates";
const STAGING_DOWNLOAD: &str = "https://static.staging.crates.io/crates";

fn run_publish(schema_file: &str, yes: bool) -> Result<(), CliError> {
    let token = std::env::var("STYX_STAGING_TOKEN").ok();
    if token.is_none() {
        return Err(CliError::Usage(
            "STYX_STAGING_TOKEN environment variable not set".into(),
        ));
    }

    let schema_content = std::fs::read_to_string(schema_file)
        .map_err(|e| CliError::Io(io::Error::new(e.kind(), format!("{schema_file}: {e}"))))?;

    let local_tree = styx_tree::parse(&schema_content)
        .map_err(|e| CliError::Parse(format!("invalid schema: {e}")))?;

    let name = extract_meta_field(&local_tree, "crate").ok_or_else(|| {
        CliError::Usage("schema must have meta.crate field for publishing".into())
    })?;

    let (version, _changes) = match fetch_latest_version(&name) {
        Ok(latest_version) => {
            eprintln!("Found {name}@{latest_version} on staging.crates.io");
            eprintln!();

            let baseline_content = fetch_crate_schema(&name, &latest_version)?;
            let baseline_tree = styx_tree::parse(&baseline_content)
                .map_err(|e| CliError::Parse(format!("invalid baseline schema: {e}")))?;

            let changes = compare_schemas(&baseline_tree, &local_tree);

            if changes.breaking.is_empty()
                && changes.additive.is_empty()
                && changes.patch.is_empty()
            {
                eprintln!("No changes detected from {latest_version}.");
                return Err(CliError::Usage("nothing to publish".into()));
            }

            if !changes.breaking.is_empty() {
                eprintln!("Breaking changes:");
                for change in &changes.breaking {
                    eprintln!("  - {change}");
                }
            }

            if !changes.additive.is_empty() {
                eprintln!("Additive changes:");
                for change in &changes.additive {
                    eprintln!("  + {change}");
                }
            }

            if !changes.patch.is_empty() {
                eprintln!("Patch changes:");
                for change in &changes.patch {
                    eprintln!("  ~ {change}");
                }
            }

            let next_version = calculate_next_version(&latest_version, &changes)?;
            eprintln!();
            eprintln!("Version bump: {latest_version} -> {next_version}");

            (next_version, Some(changes))
        }
        Err(_) => {
            eprintln!("No existing version found - this will be the first publish.");
            ("0.1.0".to_string(), None)
        }
    };

    eprintln!();

    if !yes {
        eprint!("Publish {name}@{version} to staging.crates.io? [y/N] ");
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            eprintln!("Aborted.");
            return Ok(());
        }
    }

    let temp_dir = std::env::temp_dir().join(format!("styx-publish-{name}-{}", std::process::id()));
    std::fs::create_dir_all(temp_dir.join("src"))?;
    std::fs::create_dir_all(temp_dir.join(".cargo"))?;

    let cargo_toml = generate_cargo_toml(&name, &version);
    std::fs::write(temp_dir.join("Cargo.toml"), cargo_toml)?;

    let lib_rs = generate_lib_rs(&name);
    std::fs::write(temp_dir.join("src/lib.rs"), lib_rs)?;

    let readme = generate_readme(&name);
    std::fs::write(temp_dir.join("README.md"), readme)?;

    std::fs::write(temp_dir.join("schema.styx"), &schema_content)?;

    let cargo_config = format!(
        r#"[registries.staging]
index = "{STAGING_INDEX}"
"#
    );
    std::fs::write(temp_dir.join(".cargo/config.toml"), cargo_config)?;

    eprintln!("Publishing {name}@{version}...");

    let mut cmd = std::process::Command::new("cargo");
    cmd.arg("publish")
        .arg("--registry")
        .arg("staging")
        .current_dir(&temp_dir);

    if let Some(token) = token {
        cmd.env("CARGO_REGISTRIES_STAGING_TOKEN", token);
    }

    let status = cmd.status().map_err(|e| {
        CliError::Io(io::Error::new(
            e.kind(),
            format!("failed to run cargo publish: {e}"),
        ))
    })?;

    let _ = std::fs::remove_dir_all(&temp_dir);

    if status.success() {
        eprintln!("Published {name}@{version} to staging.crates.io");
        Ok(())
    } else {
        Err(CliError::Usage(format!(
            "cargo publish failed with exit code: {}",
            status.code().unwrap_or(-1)
        )))
    }
}

fn extract_meta_field(value: &Value, field: &str) -> Option<String> {
    if let Some(Payload::Object(obj)) = &value.payload {
        for entry in &obj.entries {
            if entry.key.as_str() == Some("meta")
                && let Some(Payload::Object(meta_obj)) = &entry.value.payload
            {
                for meta_entry in &meta_obj.entries {
                    if meta_entry.key.as_str() == Some(field)
                        && let Some(Payload::Scalar(s)) = &meta_entry.value.payload
                    {
                        return Some(s.text.clone());
                    }
                }
            }
        }
    }
    None
}

fn calculate_next_version(current: &str, changes: &SchemaChanges) -> Result<String, CliError> {
    let parts: Vec<&str> = current.split('.').collect();
    if parts.len() != 3 {
        return Err(CliError::Parse(format!("invalid version: {current}")));
    }

    let major: u64 = parts[0]
        .parse()
        .map_err(|_| CliError::Parse(format!("invalid major: {}", parts[0])))?;
    let minor: u64 = parts[1]
        .parse()
        .map_err(|_| CliError::Parse(format!("invalid minor: {}", parts[1])))?;
    let patch: u64 = parts[2]
        .parse()
        .map_err(|_| CliError::Parse(format!("invalid patch: {}", parts[2])))?;

    let (new_major, new_minor, new_patch) = if !changes.breaking.is_empty() {
        if major == 0 {
            (0, minor + 1, 0)
        } else {
            (major + 1, 0, 0)
        }
    } else if !changes.additive.is_empty() {
        (major, minor + 1, 0)
    } else {
        (major, minor, patch + 1)
    };

    Ok(format!("{new_major}.{new_minor}.{new_patch}"))
}

// ============================================================================
// Diff command
// ============================================================================

fn run_diff(schema_file: &str, crate_name: &str, baseline: Option<&str>) -> Result<(), CliError> {
    let local_content = std::fs::read_to_string(schema_file)
        .map_err(|e| CliError::Io(io::Error::new(e.kind(), format!("{schema_file}: {e}"))))?;

    let local_tree = styx_tree::parse(&local_content)
        .map_err(|e| CliError::Parse(format!("invalid schema: {e}")))?;

    let version = match baseline {
        Some(v) => v.to_string(),
        None => fetch_latest_version(crate_name)?,
    };

    eprintln!("Comparing against {crate_name}@{version}...");

    let baseline_content = fetch_crate_schema(crate_name, &version)?;

    let baseline_tree = styx_tree::parse(&baseline_content)
        .map_err(|e| CliError::Parse(format!("invalid baseline schema: {e}")))?;

    let changes = compare_schemas(&baseline_tree, &local_tree);

    if changes.breaking.is_empty() && changes.additive.is_empty() && changes.patch.is_empty() {
        eprintln!("No changes detected.");
        return Ok(());
    }

    if !changes.breaking.is_empty() {
        eprintln!("\nBreaking changes (require major bump):");
        for change in &changes.breaking {
            eprintln!("  - {change}");
        }
    }

    if !changes.additive.is_empty() {
        eprintln!("\nAdditive changes (require minor bump):");
        for change in &changes.additive {
            eprintln!("  + {change}");
        }
    }

    if !changes.patch.is_empty() {
        eprintln!("\nPatch changes:");
        for change in &changes.patch {
            eprintln!("  ~ {change}");
        }
    }

    let bump = if !changes.breaking.is_empty() {
        "major"
    } else if !changes.additive.is_empty() {
        "minor"
    } else {
        "patch"
    };
    eprintln!("\nSuggested bump: {bump}");

    Ok(())
}

fn fetch_latest_version(crate_name: &str) -> Result<String, CliError> {
    let url = format!("{STAGING_API}/{crate_name}");

    let output = std::process::Command::new("curl")
        .args(["-sfL", &url])
        .output()
        .map_err(|e| CliError::Io(io::Error::new(e.kind(), format!("curl failed: {e}"))))?;

    if !output.status.success() {
        return Err(CliError::Usage(format!(
            "crate {crate_name} not found on staging.crates.io"
        )));
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| CliError::Parse(format!("invalid JSON from crates.io: {e}")))?;

    json["crate"]["max_version"]
        .as_str()
        .map(String::from)
        .ok_or_else(|| CliError::Parse("could not find max_version in response".into()))
}

fn fetch_crate_schema(crate_name: &str, version: &str) -> Result<String, CliError> {
    let url = format!("{STAGING_DOWNLOAD}/{crate_name}/{version}/download");
    let temp_dir =
        std::env::temp_dir().join(format!("styx-diff-{}-{}", crate_name, std::process::id()));
    std::fs::create_dir_all(&temp_dir)?;

    let status = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!(
            "curl -sfL '{}' | tar xzf - -C '{}'",
            url,
            temp_dir.display()
        ))
        .status()
        .map_err(|e| CliError::Io(io::Error::new(e.kind(), format!("download failed: {e}"))))?;

    if !status.success() {
        let _ = std::fs::remove_dir_all(&temp_dir);
        return Err(CliError::Usage(format!(
            "failed to download {crate_name}@{version}"
        )));
    }

    let schema_path = temp_dir.join(format!("{crate_name}-{version}/schema.styx"));
    let content = std::fs::read_to_string(&schema_path).map_err(|e| {
        let _ = std::fs::remove_dir_all(&temp_dir);
        CliError::Io(io::Error::new(
            e.kind(),
            format!("schema.styx not found in crate: {e}"),
        ))
    })?;

    let _ = std::fs::remove_dir_all(&temp_dir);
    Ok(content)
}

#[derive(Default)]
struct SchemaChanges {
    breaking: Vec<String>,
    additive: Vec<String>,
    patch: Vec<String>,
}

fn compare_schemas(baseline: &Value, local: &Value) -> SchemaChanges {
    let mut changes = SchemaChanges::default();

    let baseline_schema = extract_schema_map(baseline);
    let local_schema = extract_schema_map(local);

    for name in baseline_schema.keys() {
        if !local_schema.contains_key(name) {
            let type_name = name.as_deref().unwrap_or("(root)");
            changes.breaking.push(format!("removed type `{type_name}`"));
        }
    }

    for name in local_schema.keys() {
        if !baseline_schema.contains_key(name) {
            let type_name = name.as_deref().unwrap_or("(root)");
            changes.additive.push(format!("added type `{type_name}`"));
        }
    }

    for (name, baseline_type) in &baseline_schema {
        if let Some(local_type) = local_schema.get(name) {
            let type_name = name.as_deref().unwrap_or("(root)");
            compare_type_definitions(type_name, baseline_type, local_type, &mut changes);
        }
    }

    changes
}

fn extract_schema_map(value: &Value) -> std::collections::HashMap<Option<String>, &Value> {
    let mut map = std::collections::HashMap::new();

    if let Some(Payload::Object(obj)) = &value.payload {
        for entry in &obj.entries {
            if entry.key.as_str() == Some("schema")
                && let Some(Payload::Object(schema_obj)) = &entry.value.payload
            {
                for schema_entry in &schema_obj.entries {
                    let key = if schema_entry.key.is_unit() {
                        None
                    } else {
                        schema_entry.key.as_str().map(String::from)
                    };
                    map.insert(key, &schema_entry.value);
                }
            }
        }
    }

    map
}

fn compare_type_definitions(
    type_name: &str,
    baseline: &Value,
    local: &Value,
    changes: &mut SchemaChanges,
) {
    let baseline_tag = baseline.tag.as_ref().map(|t| t.name.as_str());
    let local_tag = local.tag.as_ref().map(|t| t.name.as_str());

    if baseline_tag != local_tag {
        changes.breaking.push(format!(
            "type `{type_name}` changed from @{} to @{}",
            baseline_tag.unwrap_or("(none)"),
            local_tag.unwrap_or("(none)")
        ));
        return;
    }

    if baseline_tag == Some("object") {
        compare_object_fields(type_name, baseline, local, changes);
    }

    if baseline_tag == Some("enum") {
        compare_enum_variants(type_name, baseline, local, changes);
    }
}

fn compare_object_fields(
    type_name: &str,
    baseline: &Value,
    local: &Value,
    changes: &mut SchemaChanges,
) {
    let baseline_fields = extract_object_fields(baseline);
    let local_fields = extract_object_fields(local);

    for field_name in baseline_fields.keys() {
        if !local_fields.contains_key(field_name) {
            changes
                .breaking
                .push(format!("removed field `{field_name}` from `{type_name}`"));
        }
    }

    for (field_name, field_type) in &local_fields {
        if !baseline_fields.contains_key(field_name) {
            let is_optional = is_optional_field(field_type);
            if is_optional {
                changes.additive.push(format!(
                    "added optional field `{field_name}` to `{type_name}`"
                ));
            } else {
                changes.breaking.push(format!(
                    "added required field `{field_name}` to `{type_name}`"
                ));
            }
        }
    }
}

fn extract_object_fields(value: &Value) -> std::collections::HashMap<String, &Value> {
    let mut fields = std::collections::HashMap::new();

    if let Some(Payload::Object(obj)) = &value.payload {
        for entry in &obj.entries {
            if let Some(name) = entry.key.as_str() {
                fields.insert(name.to_string(), &entry.value);
            }
        }
    }

    fields
}

fn is_optional_field(value: &Value) -> bool {
    value
        .tag
        .as_ref()
        .map(|t| t.name == "optional" || t.name == "default")
        .unwrap_or(false)
}

fn compare_enum_variants(
    type_name: &str,
    baseline: &Value,
    local: &Value,
    changes: &mut SchemaChanges,
) {
    let baseline_variants = extract_enum_variants(baseline);
    let local_variants = extract_enum_variants(local);

    for variant in &baseline_variants {
        if !local_variants.contains(variant) {
            changes
                .breaking
                .push(format!("removed variant `{variant}` from `{type_name}`"));
        }
    }

    for variant in &local_variants {
        if !baseline_variants.contains(variant) {
            changes
                .additive
                .push(format!("added variant `{variant}` to `{type_name}`"));
        }
    }
}

fn extract_enum_variants(value: &Value) -> Vec<String> {
    let mut variants = Vec::new();

    if let Some(Payload::Object(obj)) = &value.payload {
        for entry in &obj.entries {
            if let Some(name) = entry.key.as_str() {
                variants.push(name.to_string());
            }
        }
    }

    variants
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highlight_styx_produces_ansi_codes() {
        let source = "name value";
        let highlighted = highlight_styx(source);

        // Should contain ANSI escape codes
        assert!(
            highlighted.contains("\x1b["),
            "Highlighted output should contain ANSI escape codes"
        );
        assert!(
            highlighted.contains(ansi::RESET),
            "Highlighted output should contain reset code"
        );
    }

    #[test]
    fn test_highlight_styx_preserves_content() {
        let source = "name value\nother 123";
        let highlighted = highlight_styx(source);

        // Strip ANSI codes and verify content is preserved
        let stripped = strip_ansi_codes(&highlighted);
        assert_eq!(stripped, source);
    }

    #[test]
    fn test_highlight_different_token_types() {
        let source = r#"/// doc comment
// line comment
key "string value"
other @type{inner value}"#;
        let highlighted = highlight_styx(source);

        // Should have multiple different colors
        assert!(highlighted.contains(ansi::DOC_COMMENT));
        assert!(highlighted.contains(ansi::COMMENT));
        assert!(highlighted.contains(ansi::PROPERTY));
        assert!(highlighted.contains(ansi::STRING));
        assert!(highlighted.contains(ansi::OPERATOR)); // @ symbol
        assert!(highlighted.contains(ansi::TYPE)); // type in value position
    }

    #[test]
    fn test_highlight_empty_source() {
        let source = "";
        let highlighted = highlight_styx(source);
        assert_eq!(highlighted, "");
    }

    #[test]
    fn test_highlight_whitespace_only() {
        let source = "   \n\n   ";
        let highlighted = highlight_styx(source);
        assert_eq!(highlighted, source);
    }

    /// Helper to strip ANSI escape codes for testing
    fn strip_ansi_codes(s: &str) -> String {
        let mut result = String::new();
        let mut chars = s.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '\x1b' {
                // Skip until we find 'm' (end of ANSI sequence)
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next == 'm' {
                        break;
                    }
                }
            } else {
                result.push(c);
            }
        }

        result
    }
}
