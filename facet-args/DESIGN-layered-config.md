# Design: Layered Configuration for facet-args

## Overview

This document proposes extending `facet-args` to support layered configuration
from multiple sources, following the standard precedence:

1. **CLI flags** (highest priority)
2. **Environment variables**
3. **Config file(s)**
4. **Built-in defaults** (lowest priority)

## Motivation

Currently, `facet-args` only handles CLI argument parsing. Real-world applications
typically need configuration from multiple sources:

- **Secrets** (API keys, passwords) → environment variables
- **Per-environment settings** → config files
- **User overrides** → CLI flags
- **Sensible defaults** → code

The error messages for missing configuration are often terrible:

```
Auth config environment variables must be set: NotPresent
```

We can do much better by:
- Showing exactly which variables are missing
- Showing what each variable is for (from doc comments)
- Reporting unused/typo'd environment variables

## Design Goals

1. **Strong defaults** - Minimal annotation burden
2. **Good error messages** - Show what's missing, what's unused, where values came from
3. **Composable** - Mix CLI args, env vars, and config files naturally
4. **Traceable** - Know where each value came from (for debugging)
5. **Pluggable** - Config file format is not hardcoded (JSON built-in, others via trait)
6. **Override tracking** - Track when a higher-priority layer overrides a lower one
7. **Config dump** - Ability to dump resolved config (with sensitive fields redacted)
8. **Validation** - Integrate with `facet-validate` from day one
9. **Typo detection** - Use `strsim` to suggest corrections for unknown keys
10. **Strict mode** - Error immediately on unknown keys (configurable per-layer)
11. **Rich diagnostics** - Use `ariadne` for beautiful error reporting in config files
12. **Paths** - Use `camino` (`Utf8PathBuf`) for all paths

## Proposed API

### Core Concept: The `#[facet(args::config)]` Field

The top-level struct parsed by `facet-args` is a regular CLI args struct.
It contains ONE special field marked with `#[facet(args::config)]` which
represents the layered configuration block:

```rust
use facet::Facet;
use facet_args as args;

#[derive(Facet, Debug)]
struct Args {
    /// Show version and exit
    #[facet(args::named, args::short = 'V')]
    version: bool,

    /// Server configuration (layered: CLI > env > config file > defaults)
    #[facet(args::config, env_prefix = "REEF")]
    config: ServerConfig,
}

#[derive(Facet, Debug)]
struct ServerConfig {
    /// Port to listen on
    port: u16,
    
    /// Database connection string
    database_url: String,
    
    /// Log level
    #[facet(default = "info")]
    log_level: String,
    
    /// SMTP configuration
    smtp: SmtpConfig,
}

#[derive(Facet, Debug)]
struct SmtpConfig {
    /// SMTP server host
    host: String,
    
    /// SMTP server port
    #[facet(default = 587)]
    port: u16,
}
```

### Implicit Config File Flag

The `#[facet(args::config)]` field automatically gets a CLI flag for specifying
the config file path, using the field name:

- `--{field_name} <PATH>` or `-c <PATH>` for the config file
- `--{field_name}.{key} <VALUE>` for overrides

So with `config: ServerConfig`, you get:

```bash
# Load config from file
my_app --config ./config.json

# Or with short flag
my_app -c ./config.json

# Override specific values via CLI (dotted paths)
my_app --config ./config.json --config.port 8080 --config.smtp.host smtp.example.com

# Or use env vars (no file needed)
REEF__PORT=8080 REEF__SMTP__HOST=smtp.example.com my_app

# Or combine: file as base, env overrides, CLI overrides on top
REEF__LOG_LEVEL=debug my_app -c ./config.json --config.port 9000
```

If you prefer a different field name:

```rust
#[facet(args::config, env_prefix = "REEF")]
settings: ServerConfig,  // --settings <PATH>, --settings.port 8080
```

### Layer Resolution

For `config.port`, the value is resolved in order:

1. `--config.port 8080` from CLI
2. `REEF__PORT` from environment
3. `port` from config file (if `--config <PATH>` was provided)
4. `#[facet(default = ...)]` value (if any)
5. Error: missing required configuration

### Parsing Flow

```rust
fn main() -> Result<(), facet_args::Error> {
    // Simple API (backwards compat for CLI-only parsing)
    let args: Args = facet_args::from_slice(&["--version"])?;
    
    // Full builder API for layered config, grouped by concern
    let args: Args = facet_args::builder()
        .cli(|cli| cli
            .args(std::env::args_os().skip(1))
            .strict()
        )
        .env(|env| env
            .prefix("REEF")
            .strict()
        )
        .file(|file| file
            .format(JsonFormat)
            .format(StyxFormat)
            .strict()
        )
        .build()?;
    
    // The builder:
    // 1. Parses CLI args (version, --config path, --config.* overrides)
    // 2. For the #[facet(args::config)] field:
    //    a. If --config was provided, load and parse that file
    //    b. Read all REEF__* env vars
    //    c. Apply --config.* CLI overrides
    //    d. Merge layers: CLI > env > file > defaults
    //    e. Report any missing required fields
    //    f. Report any unused keys (warnings or errors in strict mode)
    
    if args.version {
        println!("my_app v1.0.0");
        return Ok(());
    }
    
    println!("Listening on port {}", args.config.port);
    Ok(())
}
```

## Environment Variable Naming

Given `env_prefix = "REEF"` and nested structs:

```rust
struct ServerConfig {
    port: u16,
    smtp: SmtpConfig,
}

struct SmtpConfig {
    host: String,
    connection_timeout: u64,
}
```

The env var names are:
- `REEF__PORT`
- `REEF__SMTP__HOST`
- `REEF__SMTP__CONNECTION_TIMEOUT`

Rules:
- Prefix + field path
- All SCREAMING_SNAKE_CASE
- Double underscore (`__`) as separator (to allow single `_` in field names)

## Config File Format (Pluggable)

Built-in support for JSON. Other formats implement a trait:

```rust
/// Trait for config file parsers
pub trait ConfigFormat {
    /// File extensions this format handles (e.g., ["json"], ["styx"])
    fn extensions(&self) -> &[&str];
    
    /// Parse file contents into a SpannedValue
    fn parse(&self, contents: &str) -> Result<SpannedValue, ConfigFormatError>;
}
```

### SpannedValue

`SpannedValue` wraps a `Value` with source location information. It has a
custom `Facet` impl that delegates to the inner `Value`'s `DynamicValueVTable`.

```rust
use facet_value::Value;

/// A dynamic value with source location tracking.
/// Custom Facet impl delegates to inner Value's DynamicValueVTable.
pub struct SpannedValue {
    pub value: Value,
    pub span: Span,
}

/// Source location in a config file
#[derive(Clone, Copy, Debug)]
pub struct Span {
    pub start: usize,  // byte offset
    pub end: usize,    // byte offset
}
```

This enables rich error messages with source locations, rendered using `ariadne`:

```
error: invalid value for 'port'
   ╭─[config.json:12:14]
   │
12 │     "port": "not-a-number"
   │             ───────┬──────
   │                    ╰── expected u16, got string
───╯
```

### Registering Custom Formats

```rust
use facet_styx::StyxFormat;
use camino::Utf8PathBuf;

let args: Args = facet_args::builder()
    .env(|env| env.prefix("REEF"))
    .file(|file| file
        .format(JsonFormat)
        .format(StyxFormat)  // Now .styx files work
        .default_paths(&[
            "./config.json",                    // Check first (project local)
            "~/.config/myapp/config.json",      // Then user config
            "/etc/myapp/config.json",           // Then system config
        ])
    )
    .build()?;
```

**Default paths behavior:**
- Checked in order, first one that exists wins (no merging across files)
- If `--config <PATH>` is explicitly provided, it overrides default paths entirely
- Provenance tracks which file was actually used

## Provenance Tracking

Track where each value came from using `HashMap<Path, Provenance>`.

We use `camino::Utf8PathBuf` for all paths, and file provenance is refcounted
to avoid cloning paths for every field that came from the same file:

```rust
use std::sync::Arc;
use camino::Utf8PathBuf;

/// Information about a loaded config file (shared across all values from that file)
pub struct ConfigFile {
    pub path: Utf8PathBuf,
    pub contents: String,  // Kept for error reporting with ariadne
}

pub enum Provenance {
    Cli(String),              // The CLI arg string, e.g. "--config.port"
    Env(String),              // The env var name, e.g. "REEF__PORT"
    File {
        file: Arc<ConfigFile>,  // Refcounted - shared across all values from this file
        key_path: String,       // e.g. "smtp.host"
        span: Span,             // Location in file for error reporting
    },
    Default,                  // From #[facet(default = ...)]
}

pub struct ConfigResult<T> {
    pub value: T,
    pub provenance: HashMap<String, Provenance>,  // "config.port" -> Provenance::Env("REEF__PORT")
}
```

For debugging, print where each value came from:

```rust
let result = facet_args::builder()
    .cli(|cli| cli.args(std::env::args_os().skip(1)))
    .env(|env| env.prefix("REEF"))
    .build_traced()?;

for (path, source) in &result.provenance {
    match source {
        Provenance::Cli(arg) => println!("{} = ... (from CLI: {})", path, arg),
        Provenance::Env(var) => println!("{} = ... (from env: {})", path, var),
        Provenance::File { file, key_path, .. } => {
            println!("{} = ... (from {}: {})", path, file.path, key_path)
        }
        Provenance::Default => println!("{} = ... (from default)", path),
    }
}
```

## Override Tracking

Track when a value from a higher-priority layer overrides a lower one:

```rust
pub struct Override {
    pub path: String,
    pub winner: Provenance,
    pub loser: Provenance,
}

pub struct ConfigResult<T> {
    pub value: T,
    pub provenance: HashMap<String, Provenance>,
    pub overrides: Vec<Override>,  // What got overridden by what
}
```

This enables warnings like:

```
note: CLI argument --config.port=9000 overrides env var REEF__PORT=8080
```

## Config Dump

Dump the resolved configuration for debugging/inspection. Fields marked with
`#[facet(sensitive)]` are automatically redacted:

```rust
let args: Args = facet_args::from_env()?;

// Dump config to stderr (respects sensitive fields)
facet_args::dump_config(&args.config)?;
```

Output:

```
Resolved configuration:
  port = 8080 (from env: REEF__PORT)
  database_url = "postgres://localhost/mydb" (from file: config.json)
  smtp.host = "smtp.example.com" (from CLI: --config.smtp.host)
  smtp.port = 587 (from default)
  smtp.password = *** (from env: REEF__SMTP__PASSWORD) [sensitive]
  log_level = "info" (from default)
```

## Validation (facet-validate)

Validation runs after all layers are merged, using `facet-validate`:

```rust
use facet::Facet;
use facet_validate as validate;

#[derive(Facet, Debug)]
struct ServerConfig {
    /// Port to listen on
    #[facet(validate::min = 1, validate::max = 65535)]
    port: u16,
    
    /// Database URL
    #[facet(validate::min_length = 1)]
    database_url: String,
    
    /// Log level
    #[facet(validate::regex = r"^(trace|debug|info|warn|error)$")]
    log_level: String,
}
```

Validation errors include provenance:

```
error: validation failed

  config.port = 0 (from env: REEF__PORT)
  │ Port must be in range 1..=65535
  
  config.log_level = "verbose" (from file: config.json:8:14)
  │ Must be one of: trace, debug, info, warn, error
```

## Error Messages

### Missing Required Variables

```
error: missing required configuration

The following values are required but not set:

  REEF__DATABASE_URL (String)
  │ Database connection string
  │ 
  │ Can be set via:
  │   • CLI: --config.database-url <VALUE>
  │   • Env: REEF__DATABASE_URL=<VALUE>
  │   • Config file: { "database_url": "<VALUE>" }
  
  REEF__SMTP__HOST (String)
  │ SMTP server host
  │ 
  │ Can be set via:
  │   • CLI: --config.smtp.host <VALUE>
  │   • Env: REEF__SMTP__HOST=<VALUE>
  │   • Config file: { "smtp": { "host": "<VALUE>" } }

hint: use --help to see all options
```

### Unused Environment Variables

When we see `REEF__*` variables that don't match any field:

```
warning: unused environment variables with prefix REEF__

  REEF__DATABSE_URL (did you mean REEF__DATABASE_URL?)
  REEF__STMP__HOST (did you mean REEF__SMTP__HOST?)
  REEF__UNKNOWN_FIELD
```

## Typo Detection (strsim)

Use `strsim` (string similarity) to suggest corrections for unknown keys.
This applies to all layers:

- **Env vars**: `REEF__DATABSE_URL` → "did you mean REEF__DATABASE_URL?"
- **Config file keys**: `databse_url` → "did you mean database_url?"
- **CLI args**: `--config.databse-url` → "did you mean --config.database-url?"

Suggestions are shown when the edit distance is small enough to be a likely typo.

## Strict Mode

By default, unknown keys generate warnings. Strict mode can be enabled per-layer:

```rust
let args: Args = facet_args::builder()
    .cli(|cli| cli.args(std::env::args_os().skip(1)).strict())
    .env(|env| env.prefix("REEF").strict())
    .file(|file| file.format(JsonFormat))  // not strict - allow forward-compat
    .build()?;
```

This allows fine-grained control. For example, you might want:
- `.env(|e| e.strict())` in production (catch typos in deployment)
- `.file(|f| f...)` without `.strict()` to allow forward-compatible config files

Strict mode errors on unknown keys:

```
error: unknown configuration key

  REEF__DATABSE_URL
  │ Unknown environment variable with prefix REEF__
  │ 
  │ Did you mean: REEF__DATABASE_URL?

error: unknown configuration key

  config.json:5:3
    │
  5 │   "databse_url": "postgres://..."
    │   ^^^^^^^^^^^^ Unknown key in config file
    │ 
    │ Did you mean: database_url?

error: unknown argument

  --config.databse-url
  │ Unknown configuration path
  │ 
  │ Did you mean: --config.database-url?
```

This is useful for CI/production to catch typos that would otherwise silently
use default values.

## Builder API

All configuration goes through `facet_args::builder()`. The only standalone function
is `facet_args::from_slice()` for backwards compatibility (CLI-only parsing):

```rust
// Backwards compat: CLI args only, no env/file support
let args: Args = facet_args::from_slice(&["--verbose", "file.txt"])?;

// Full layered config via builder, grouped by concern
let args: Args = facet_args::builder()
    .cli(|cli| cli
        .args(std::env::args_os().skip(1))
        .strict()
    )
    .env(|env| env
        .prefix("MYAPP")
        .strict()
    )
    .file(|file| file
        .format(JsonFormat)
        .format(StyxFormat)
        .strict()
    )
    .build()?;

// Builder with tracing enabled
let result = facet_args::builder()
    .cli(|cli| cli.args(std::env::args_os().skip(1)))
    .env(|env| env.prefix("MYAPP"))
    .build_traced()?;

println!("Config: {:?}", result.value);
for (path, prov) in &result.provenance {
    println!("  {} from {:?}", path, prov);
}
for ovr in &result.overrides {
    println!("  {} overrode {}", ovr.winner, ovr.loser);
}

## Help Generation

Help output shows both CLI flags and environment variables together. For config
file format, users should refer to the schema (Styx has built-in schema support,
JSON has JSON Schema, etc.)

```
my_app 1.0.0

USAGE:
    my_app [OPTIONS]

OPTIONS:
    -V, --version          Show version and exit
    -c, --config <PATH>    Path to config file

CONFIGURATION:
    The following can be set via CLI or environment variables.
    Priority: CLI > env > config file > defaults

    --config.port <u16> [required]
        Port to listen on
        ($REEF__PORT)

    --config.database-url <String> [required]
        Database connection string
        ($REEF__DATABASE_URL)

    --config.smtp.host <String> [required]
        SMTP server host
        ($REEF__SMTP__HOST)

    --config.smtp.port <u16> [default: 587]
        SMTP server port
        ($REEF__SMTP__PORT)

    --config.smtp.password <String> [required, sensitive]
        SMTP password
        ($REEF__SMTP__PASSWORD)
```

The `[sensitive]` marker indicates fields that are redacted in config dumps and logs.

## Sensitive Fields

Use the existing `#[facet(sensitive)]` attribute to mask values in logs/traces:

```rust
#[derive(Facet)]
struct SmtpConfig {
    host: String,
    
    #[facet(sensitive)]
    password: String,  // Shown as "***" in trace output
}
```

## Vec/List Handling in Env Vars

For `Vec<String>` fields, use comma-separated values:

```bash
REEF__ALLOWED_EMAILS=alice@example.com,bob@example.com
```

Rules:
- Split on `,`
- Trim whitespace from each element
- Use `\,` to escape a literal comma

## Required vs Optional

- `Option<T>` → optional, can be `None`
- `T` with `#[facet(default = ...)]` → optional, uses default if not set
- `T` without default → required, error if not set anywhere

```rust
struct Config {
    required: String,           // Must be set in CLI, env, or file
    optional: Option<String>,   // Can be None
    defaulted: String,          // Uses default if not set
}
```

## Implementation Notes

### Config File Path Bootstrapping

The `--{field_name}` flag (e.g., `--config`) is implicitly created for the
`#[facet(args::config)]` field. When a bare path is provided (not a dotted
override), it's interpreted as the config file path.

Parsing order:
1. Parse CLI args, separating:
   - `--config <PATH>` → config file path
   - `--config.foo.bar <VALUE>` → CLI overrides
2. If config path provided, load and parse the file → base layer
3. Read env vars with prefix → middle layer  
4. Apply CLI overrides → top layer
5. Fill defaults for any remaining unset fields
6. Error on missing required fields

### Merging Strategy

Use `facet_value::Value` for merging:

1. Start with empty `Value::Map`
2. Deep-merge config file values (if any)
3. Deep-merge env var values
4. Deep-merge CLI override values
5. Convert final `Value` to target type

"Deep merge" means nested maps are merged recursively, not replaced entirely.

## Example: Full AuthConfig

```rust
use facet::Facet;
use facet_args as args;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Facet, Debug)]
struct Args {
    #[facet(args::config, env_prefix = "REEF")]
    config: AuthConfig,
}

#[derive(Facet, Debug)]
struct AuthConfig {
    /// Comma-separated list of allowed admin emails
    allowed_emails: Vec<String>,

    /// WebAuthn relying party ID (domain)
    rp_id: String,

    /// WebAuthn relying party origin URL
    rp_origin: String,

    /// WebAuthn relying party display name
    rp_name: String,

    /// SMTP configuration
    smtp: SmtpConfig,

    /// From email address for magic links
    email_from: String,

    /// Base URL for magic links
    magic_link_base_url: String,

    /// Magic link validity duration in seconds
    #[facet(default = 900)]  // 15 minutes
    magic_link_ttl_secs: u64,

    /// Session validity duration in seconds
    #[facet(default = 604800)]  // 7 days
    session_ttl_secs: u64,
}

#[derive(Facet, Debug)]
struct SmtpConfig {
    /// SMTP server host
    host: String,

    /// SMTP server port
    #[facet(default = 587)]
    port: u16,

    /// SMTP username
    username: String,

    /// SMTP password
    #[facet(sensitive)]
    password: String,
}
```

Running with missing config:

```
$ my_app

error: missing required configuration

The following values are required but not set:

  REEF__ALLOWED_EMAILS (Vec<String>)
  │ Comma-separated list of allowed admin emails
  │ 
  │ Can be set via:
  │   • CLI: --config.allowed-emails <VALUE>
  │   • Env: REEF__ALLOWED_EMAILS=<VALUE>
  │   • Config file: { "allowed_emails": [...] }

  REEF__RP_ID (String)
  │ WebAuthn relying party ID (domain)
  │ 
  │ Can be set via:
  │   • CLI: --config.rp-id <VALUE>
  │   • Env: REEF__RP_ID=<VALUE>
  │   • Config file: { "rp_id": "<VALUE>" }

  ... (and 6 more)

hint: use --help to see all options
```

## Related Work

- **clap** - CLI parsing with env var support via `#[arg(env = "VAR")]`
- **config-rs** - Layered config, but not integrated with CLI parsing
- **figment** - Layered config with providers, used by Rocket
- **envy** - Env-to-struct deserialization via serde

Our advantage: Integration with facet's reflection system means we can:
- Use doc comments as help text automatically
- Generate better error messages with field-level context
- Share type definitions between CLI, env, and config
- No proc macro needed beyond `#[derive(Facet)]`
