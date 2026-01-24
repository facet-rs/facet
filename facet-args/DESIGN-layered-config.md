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

## Proposed API

### Basic Usage (env vars only)

For simple cases where you just want env vars with a prefix:

```rust
use facet::Facet;
use facet_args::ConfigBuilder;

#[derive(Facet, Debug)]
struct AuthConfig {
    /// Comma-separated list of allowed admin emails
    allowed_emails: Vec<String>,
    
    /// WebAuthn relying party ID (domain)
    rp_id: String,
    
    /// SMTP server port
    #[facet(default = 587)]
    smtp_port: u16,
}

fn main() -> Result<(), facet_args::Error> {
    let config: AuthConfig = ConfigBuilder::new()
        .env_prefix("REEF")
        .build()?;
    
    // Reads from:
    //   REEF__ALLOWED_EMAILS
    //   REEF__RP_ID  
    //   REEF__SMTP_PORT (optional, defaults to 587)
}
```

### Full Layered Config (CLI + env + config file)

For CLI tools that need all layers:

```rust
use facet::Facet;
use facet_args::{self as args, ConfigBuilder};

#[derive(Facet, Debug)]
struct ServerArgs {
    /// Show version and exit
    #[facet(args::named, args::short = 'V')]
    version: bool,

    /// Server configuration
    #[facet(args::config)]
    server: ServerConfig,
}

#[derive(Facet, Debug)]
struct ServerConfig {
    /// Port to listen on
    #[facet(args::named, args::short = 'p')]
    port: u16,
    
    /// Database connection string
    database_url: String,
    
    /// Log level
    #[facet(default = "info")]
    log_level: String,
}

fn main() -> Result<(), facet_args::Error> {
    let args: ServerArgs = ConfigBuilder::new()
        .cli_args(std::env::args_os().skip(1))
        .env_prefix("MYAPP")
        .config_file("config.toml")  // optional
        .build()?;
    
    // For `server.port`, checks in order:
    //   1. --port / -p from CLI
    //   2. MYAPP__SERVER__PORT from env
    //   3. server.port from config.toml
    //   4. default value (if any)
}
```

### Env-Only Structs (no CLI args)

Structs without any `args::named` or `args::positional` annotations are
treated as "config-only" - they won't generate CLI flags:

```rust
#[derive(Facet, Debug)]
struct DatabaseConfig {
    /// PostgreSQL connection URL
    url: String,
    
    /// Connection pool size
    #[facet(default = 10)]
    pool_size: u32,
}
// Only reads from env vars and config file, no CLI flags
```

## Environment Variable Naming

Given prefix `MYAPP` and nested structs:

```rust
struct Config {
    database: DatabaseConfig,
}

struct DatabaseConfig {
    connection_url: String,
}
```

The env var name is: `MYAPP__DATABASE__CONNECTION_URL`

Rules:
- Prefix + struct path + field name
- All SCREAMING_SNAKE_CASE
- Double underscore (`__`) as separator (to allow single `_` in names)

## Error Messages

### Missing Required Variables

```
error: missing required configuration

The following values are required but not set:

  REEF__ALLOWED_EMAILS (Vec<String>)
  │ Comma-separated list of allowed admin emails
  │ 
  │ Set via: environment variable or config file
  
  REEF__RP_ID (String)
  │ WebAuthn relying party ID (domain)
  │ 
  │ Set via: environment variable or config file

hint: you can also use --help to see CLI options
```

### Unused Environment Variables

When we see `REEF__*` variables that don't match any field:

```
warning: unused environment variables with prefix REEF__

  REEF__ALLOWED_EMAIL (did you mean REEF__ALLOWED_EMAILS?)
  REEF__RP_IDD (did you mean REEF__RP_ID?)
  REEF__UNKNOWN_FIELD
```

### Value Origin Tracing

For debugging, show where each value came from:

```rust
let config = ConfigBuilder::new()
    .env_prefix("MYAPP")
    .config_file("config.toml")
    .trace(true)  // Enable tracing
    .build()?;
```

Outputs:
```
configuration loaded:
  port = 8080 (from CLI: --port 8080)
  database_url = "postgres://..." (from env: MYAPP__DATABASE_URL)
  log_level = "info" (from default)
  pool_size = 20 (from file: config.toml)
```

## Implementation Strategy

### Phase 1: Environment Variables Only (`facet-env`)

Start simple - just env var parsing with good errors:

```rust
// facet-env crate (or part of facet-args)
let config: AuthConfig = facet_env::from_prefix("REEF")?;
```

This gives us:
- ✅ Good error messages for missing vars
- ✅ Unused variable detection  
- ✅ Type conversion with helpful errors
- ✅ Doc comments as help text

### Phase 2: Layered Config

Extend `ConfigBuilder` to support multiple layers with proper merging.

Key question: How to track provenance?

Option A: **Use `facet_value::Value` with metadata**
```rust
enum ValueSource {
    Cli(String),        // "--port 8080"
    Env(String),        // "MYAPP__PORT"
    File(PathBuf, String), // ("config.toml", "port")
    Default,
}
```

Option B: **Two-pass approach**
1. Parse each layer into `facet_value::Value`
2. Merge with precedence, tracking what overwrote what
3. Convert final `Value` to target type

### Phase 3: Config File Support

Support common formats:
- TOML (primary)
- JSON
- YAML (optional, heavier dependency)

Config file path itself can come from:
1. CLI flag (`--config path/to/config.toml`)
2. Env var (`MYAPP__CONFIG_FILE`)
3. Default locations (`./config.toml`, `~/.config/myapp/config.toml`)

## Open Questions

### 1. Nested Prefix Handling

For deeply nested structs, env var names get long:
`MYAPP__DATABASE__CONNECTION__POOL__MAX_SIZE`

Should we support a `#[facet(env_prefix = "DB")]` override?

```rust
#[derive(Facet)]
struct Config {
    #[facet(env_prefix = "DB")]  // Uses DB__* instead of MYAPP__DATABASE__*
    database: DatabaseConfig,
}
```

### 2. Secret Masking

Should we have a way to mark fields as secrets (for logging)?

```rust
#[derive(Facet)]
struct Config {
    #[facet(secret)]
    api_key: String,  // Shown as "***" in trace output
}
```

### 3. Validation

Where does validation fit in?

```rust
#[derive(Facet)]
struct Config {
    #[facet(validate = "1..=65535")]
    port: u16,
}
```

Or should validation be a separate concern (facet-validate)?

### 4. Required vs Optional

How to distinguish "required" from "optional with no default"?

```rust
struct Config {
    required: String,           // Must be set somewhere
    optional: Option<String>,   // Can be None
    defaulted: String,          // Has #[facet(default = "...")]
}
```

Current thinking: `Option<T>` means optional, everything else is required
unless it has a `default`.

### 5. List/Vec Handling in Env Vars

How to represent `Vec<String>` in an env var?

Options:
- Comma-separated: `MYAPP__EMAILS=a@b.com,c@d.com`
- JSON array: `MYAPP__EMAILS=["a@b.com","c@d.com"]`
- Repeated vars: `MYAPP__EMAILS__0=a@b.com` `MYAPP__EMAILS__1=c@d.com`

Recommendation: Comma-separated by default, with escape for literal commas.

## Related Work

- **clap** - CLI parsing with env var support via `#[arg(env = "VAR")]`
- **config-rs** - Layered config, but not integrated with CLI parsing
- **figment** - Layered config with providers, used by Rocket
- **envy** - Env-to-struct deserialization via serde

Our advantage: Integration with facet's reflection system means we can:
- Use doc comments as help text automatically
- Generate better error messages
- Share type definitions between CLI, env, and config

## Next Steps

1. [ ] Implement `facet-env` as proof of concept
2. [ ] Test with `AuthConfig` in reef
3. [ ] Design `ConfigBuilder` API in detail
4. [ ] Implement layered merging with `facet_value::Value`
5. [ ] Add config file support