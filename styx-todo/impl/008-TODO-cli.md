# Phase 008: styx-cli (Command-Line Tool)

A `jq`-like command-line tool for working with Styx documents. Supports querying, transforming, validating, and converting between formats.

## Deliverables

- `crates/styx-cli/src/main.rs` - CLI entry point
- `crates/styx-cli/src/commands/` - Subcommand implementations
- `crates/styx-cli/src/format.rs` - Format conversion
- `crates/styx-cli/src/query.rs` - Path-based querying
- `crates/styx-cli/src/output.rs` - Output formatting

## Dependencies

```toml
[dependencies]
styx-parse = { path = "../styx-parse" }
styx-tree = { path = "../styx-tree" }
styx-cst = { path = "../styx-cst" }
styx-schema = { path = "../styx-schema" }
styx-format = { path = "../styx-format" }
serde_json = "1"
serde_yaml = "0.9"
clap = { version = "4", features = ["derive"] }
anyhow = "1"
colored = "2"
```

## Command Structure

```
styx <command> [options] [file]

Commands:
  fmt       Format/pretty-print a Styx document
  check     Validate syntax and optionally schema
  query     Extract values using path expressions
  get       Get a single value by path
  set       Set a value at a path
  convert   Convert between Styx, JSON, YAML
  schema    Schema-related subcommands

Global Options:
  -h, --help     Show help
  -V, --version  Show version
  -q, --quiet    Suppress non-error output
  --color        Color output (auto, always, never)
```

## Commands

### `styx fmt` - Format Documents

```bash
# Format a file in place
styx fmt config.styx

# Format to stdout
styx fmt --stdout config.styx

# Format all .styx files in directory
styx fmt --recursive .

# Check formatting without modifying
styx fmt --check config.styx

# Format with specific style
styx fmt --indent 4 --separator comma config.styx
```

Options:
- `--stdout` - Print to stdout instead of modifying file
- `--check` - Exit with error if not formatted
- `--recursive, -r` - Process directories recursively
- `--indent N` - Indentation width (default: 4)
- `--separator` - Entry separator: `newline` (default) or `comma`
- `--compact` - Single-line output where possible

### `styx check` - Validate Documents

```bash
# Check syntax
styx check config.styx

# Check with schema
styx check --schema server.schema.styx config.styx

# Check with auto-discovered schema
styx check --schema-auto config.styx

# Check multiple files
styx check *.styx

# Output as JSON (for CI integration)
styx check --format json config.styx
```

Options:
- `--schema PATH` - Schema file to validate against
- `--schema-auto` - Auto-discover schema
- `--format` - Output format: `human` (default), `json`, `github`
- `--strict` - Treat warnings as errors

Exit codes:
- `0` - Valid
- `1` - Syntax errors
- `2` - Schema validation errors
- `3` - File not found / IO error

### `styx query` - Query Documents

Path expression syntax (similar to jq):
```bash
# Get a value
styx query '.server.host' config.styx

# Get nested value
styx query '.database.connection.pool_size' config.styx

# Get sequence element
styx query '.servers[0].host' config.styx

# Get all hosts from sequence
styx query '.servers[].host' config.styx

# Filter by condition
styx query '.servers[] | select(.enabled == true)' config.styx
```

Options:
- `--raw, -r` - Output raw strings without quotes
- `--null, -n` - Output `null` for missing paths instead of error
- `--format` - Output format: `styx`, `json`, `yaml`

### `styx get` - Get Single Value

Simpler alternative to `query` for single values:

```bash
# Get value (exits with error if missing)
styx get server.host config.styx

# Get with default
styx get server.port --default 8080 config.styx

# Get as specific type
styx get server.port --type int config.styx
```

### `styx set` - Set Value

```bash
# Set a value
styx set server.host '"localhost"' config.styx

# Set from file
styx set server.tls --file tls-config.styx config.styx

# Set and output to stdout (don't modify file)
styx set server.port 9000 --stdout config.styx

# Delete a key
styx set server.debug --delete config.styx
```

### `styx convert` - Format Conversion

```bash
# Styx to JSON
styx convert config.styx --to json

# JSON to Styx
styx convert config.json --to styx

# YAML to Styx
styx convert config.yaml --to styx

# Styx to YAML
styx convert config.styx --to yaml

# Read from stdin
cat config.json | styx convert --from json --to styx

# Output to file
styx convert config.json --to styx --output config.styx
```

Options:
- `--from` - Input format (auto-detected from extension if not specified)
- `--to` - Output format (required)
- `--output, -o` - Output file (stdout if not specified)
- `--pretty` - Pretty-print output (default for styx/yaml)
- `--compact` - Compact output

### `styx schema` - Schema Subcommands

```bash
# Validate a schema file
styx schema check server.schema.styx

# Generate schema from example document
styx schema infer config.styx > config.schema.styx

# Show schema info
styx schema info server.schema.styx

# Generate TypeScript types from schema
styx schema codegen --lang typescript server.schema.styx

# Generate Rust types from schema
styx schema codegen --lang rust server.schema.styx
```

## Implementation

### CLI Structure

```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "styx")]
#[command(about = "Command-line tool for Styx configuration files")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    
    #[arg(long, global = true, default_value = "auto")]
    color: ColorChoice,
    
    #[arg(short, long, global = true)]
    quiet: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Format Styx documents
    Fmt(FmtArgs),
    /// Validate syntax and schema
    Check(CheckArgs),
    /// Query documents with path expressions
    Query(QueryArgs),
    /// Get a single value by path
    Get(GetArgs),
    /// Set a value at a path
    Set(SetArgs),
    /// Convert between formats
    Convert(ConvertArgs),
    /// Schema operations
    Schema(SchemaArgs),
}
```

### Format Conversion

```rust
pub enum Format {
    Styx,
    Json,
    Yaml,
}

impl Format {
    pub fn detect(path: &Path) -> Option<Format> {
        match path.extension()?.to_str()? {
            "styx" => Some(Format::Styx),
            "json" => Some(Format::Json),
            "yaml" | "yml" => Some(Format::Yaml),
            _ => None,
        }
    }
}

pub fn convert(
    input: &str,
    from: Format,
    to: Format,
    options: &ConvertOptions,
) -> Result<String> {
    // Parse input to intermediate representation
    let value = match from {
        Format::Styx => parse_styx(input)?,
        Format::Json => parse_json(input)?,
        Format::Yaml => parse_yaml(input)?,
    };
    
    // Serialize to output format
    match to {
        Format::Styx => serialize_styx(&value, options),
        Format::Json => serialize_json(&value, options),
        Format::Yaml => serialize_yaml(&value, options),
    }
}
```

### Path Expressions

```rust
pub enum PathSegment {
    /// Field access: `.foo`
    Field(String),
    /// Index access: `[0]`
    Index(usize),
    /// Iterate: `[]`
    Iterate,
}

pub struct Path {
    segments: Vec<PathSegment>,
}

impl Path {
    pub fn parse(s: &str) -> Result<Path>;
    
    pub fn get<'a>(&self, doc: &'a Document) -> Option<&'a Value>;
    
    pub fn get_all<'a>(&self, doc: &'a Document) -> Vec<&'a Value>;
    
    pub fn set(&self, doc: &mut Document, value: Value) -> Result<()>;
}
```

### Output Formatting

```rust
pub struct OutputFormatter {
    color: bool,
    format: OutputFormat,
}

impl OutputFormatter {
    pub fn print_value(&self, value: &Value) {
        match self.format {
            OutputFormat::Styx => self.print_styx(value),
            OutputFormat::Json => self.print_json(value),
            OutputFormat::Yaml => self.print_yaml(value),
        }
    }
    
    pub fn print_error(&self, error: &Error) {
        if self.color {
            eprintln!("{}: {}", "error".red().bold(), error);
        } else {
            eprintln!("error: {}", error);
        }
    }
    
    pub fn print_diagnostics(&self, diagnostics: &[Diagnostic], source: &str) {
        // Pretty-print with source context and carets
        for diag in diagnostics {
            self.print_diagnostic(diag, source);
        }
    }
}
```

### Error Output (GitHub Actions Compatible)

```rust
pub fn print_github_format(diagnostics: &[Diagnostic], path: &Path) {
    for diag in diagnostics {
        let level = match diag.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Hint => "notice",
        };
        println!(
            "::{} file={},line={},col={}::{}",
            level,
            path.display(),
            diag.line,
            diag.column,
            diag.message
        );
    }
}
```

## Examples

### Pipeline Usage

```bash
# Extract all hostnames and sort them
styx query '.servers[].host' config.styx | sort | uniq

# Convert YAML to Styx and validate
cat config.yaml | styx convert --from yaml --to styx | styx check --schema app.schema.styx

# Get a value and use in shell
PORT=$(styx get server.port config.styx)
echo "Server running on port $PORT"

# Update config from CI
styx set version '"1.2.3"' config.styx
styx set build.timestamp "$(date +%s)" config.styx
```

### CI Integration

```yaml
# GitHub Actions
- name: Validate Styx configs
  run: |
    styx check --format github --schema-auto configs/*.styx
```

```yaml
# GitLab CI
validate-config:
  script:
    - styx check --format json configs/ > report.json
  artifacts:
    reports:
      codequality: report.json
```

## Testing

- Unit tests for each command
- Integration tests with temp files
- Snapshot tests for output formatting
- Round-trip tests for format conversion
- Shell completion tests
