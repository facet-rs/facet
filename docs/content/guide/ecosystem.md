+++
title = "Ecosystem Integration"
weight = 7
insert_anchor_links = "heading"
+++

Facet provides `Facet` trait implementations for many popular Rust crates via feature flags. Enable the feature, and those types work seamlessly with all facet format crates.

## Third-Party Type Support

Enable these features in your `Cargo.toml`:

```toml
[dependencies]
facet = { version = "{{ data.versions.facet }}", features = ["uuid", "chrono"] }
```

### Available Features

| Feature | Crate | Types |
|---------|-------|-------|
| `uuid` | [uuid](https://docs.rs/uuid) | `Uuid` |
| `ulid` | [ulid](https://docs.rs/ulid) | `Ulid` |
| `url` | [url](https://docs.rs/url) | `Url` |
| `chrono` | [chrono](https://docs.rs/chrono) | `DateTime<Tz>`, `NaiveDate`, `NaiveTime`, `NaiveDateTime` |
| `time` | [time](https://docs.rs/time) | `Date`, `Time`, `PrimitiveDateTime`, `OffsetDateTime`, `Duration` |
| `jiff02` | [jiff](https://docs.rs/jiff) | `Timestamp`, `Zoned`, `DateTime`, `Date`, `Time`, `Span`, `SignedDuration` |
| `camino` | [camino](https://docs.rs/camino) | `Utf8Path`, `Utf8PathBuf` |
| `bytes` | [bytes](https://docs.rs/bytes) | `Bytes`, `BytesMut` |
| `ordered-float` | [ordered-float](https://docs.rs/ordered-float) | `OrderedFloat<f32>`, `OrderedFloat<f64>`, `NotNan<f32>`, `NotNan<f64>` |
| `ruint` | [ruint](https://docs.rs/ruint) | `Uint<BITS, LIMBS>`, `Bits<BITS, LIMBS>` |

### Example: UUIDs

```rust
use facet::Facet;
use uuid::Uuid;

#[derive(Facet)]
struct User {
    id: Uuid,
    name: String,
}

let json = r#"{"id": "550e8400-e29b-41d4-a716-446655440000", "name": "Alice"}"#;
let user: User = facet_json::from_str(json)?;
```

### Example: DateTime with chrono

```rust
use facet::Facet;
use chrono::{DateTime, Utc};

#[derive(Facet)]
struct Event {
    name: String,
    timestamp: DateTime<Utc>,
}

let json = r#"{"name": "deploy", "timestamp": "2024-01-15T10:30:00Z"}"#;
let event: Event = facet_json::from_str(json)?;
```

### Example: UTF-8 Paths with camino

```rust
use facet::Facet;
use camino::Utf8PathBuf;

#[derive(Facet)]
struct Config {
    data_dir: Utf8PathBuf,
}
```

## Extended Tuple Support

By default, facet supports tuples up to 4 elements. Enable `tuples-12` for tuples up to 12 elements:

```toml
[dependencies]
facet = { version = "{{ data.versions.facet }}", features = ["tuples-12"] }
```

## Function Pointer Support

Enable `fn-ptr` for `Facet` implementations on function pointer types:

```toml
[dependencies]
facet = { version = "{{ data.versions.facet }}", features = ["fn-ptr"] }
```

## facet-args: CLI Argument Parsing

Beyond basic argument parsing, `facet-args` provides utilities for help generation and shell completions.

### Help Generation

Generate formatted help text from your type's structure and doc comments:

```rust
use facet::Facet;
use facet_args::{generate_help, HelpConfig};

/// A file processing tool.
#[derive(Facet)]
struct Args {
    /// Enable verbose output
    #[facet(args::named, args::short)]
    verbose: bool,

    /// Input file to process
    #[facet(args::positional)]
    input: String,
}

fn main() {
    let config = HelpConfig {
        program_name: Some("mytool".into()),
        version: Some("1.0.0".into()),
        ..Default::default()
    };

    println!("{}", generate_help::<Args>(&config));
}
```

### Shell Completions

Generate completion scripts for bash, zsh, and fish:

```rust
use facet_args::{generate_completions, Shell};

// Generate bash completions
let bash = generate_completions::<Args>(Shell::Bash, "mytool");
println!("{}", bash);

// Generate zsh completions
let zsh = generate_completions::<Args>(Shell::Zsh, "mytool");

// Generate fish completions
let fish = generate_completions::<Args>(Shell::Fish, "mytool");
```

Install completions by writing to the appropriate location:
- **Bash:** `~/.local/share/bash-completion/completions/mytool`
- **Zsh:** `~/.zsh/completions/_mytool`
- **Fish:** `~/.config/fish/completions/mytool.fish`

### Parsing from std::env

For quick CLI tools:

```rust
use facet_args::from_std_args;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Args = from_std_args()?;
    // ...
    Ok(())
}
```

See the [Args showcase](@/guide/showcases/args.md) for comprehensive examples including subcommands, error messages, and more.

## no_std Support

Facet works in `no_std` environments. Disable default features and enable `alloc`:

```toml
[dependencies]
facet = { version = "{{ data.versions.facet }}", default-features = false, features = ["alloc"] }
```

Some format crates also support `no_std`:
- `facet-json` — with `alloc` feature
- `facet-postcard` — with `alloc` feature
- `facet-msgpack` — with `alloc` feature
