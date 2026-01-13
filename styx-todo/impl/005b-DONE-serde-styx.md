# Phase 005b: serde_styx ✅

Serde-based serializer and deserializer for Styx, sharing core logic with `facet-styx`.

## Status: COMPLETE

All implementation steps completed:
1. ✅ Created `styx-format` crate with shared formatting logic
2. ✅ Refactored `facet-styx` to use `styx-format`
3. ✅ Created `serde_styx` crate with full serde support
4. ✅ Added cross-compatibility tests in `styx-compat-tests`

## Crate Structure

```
crates/
├── styx-format/          # Shared formatting logic
│   └── src/
│       ├── lib.rs
│       ├── options.rs    # FormatOptions
│       ├── scalar.rs     # can_be_bare, escape/unescape
│       └── writer.rs     # StyxWriter
├── facet-styx/           # Facet integration (refactored)
│   └── src/
│       ├── lib.rs
│       ├── error.rs
│       ├── parser.rs     # FormatParser impl
│       └── serializer.rs # Uses StyxWriter
├── serde_styx/           # Serde integration (new)
│   └── src/
│       ├── lib.rs
│       ├── error.rs
│       ├── de.rs         # serde::Deserializer impl
│       └── ser.rs        # serde::Serializer impl
└── styx-compat-tests/    # Cross-compatibility tests
    └── src/lib.rs
```

## API

### serde_styx

```rust
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
struct Config {
    name: String,
    port: u16,
    debug: bool,
}

// Deserialize
let config: Config = serde_styx::from_str("name myapp\nport 8080\ndebug true")?;

// Serialize
let styx = serde_styx::to_string(&config)?;

// Compact (single line)
let compact = serde_styx::to_string_compact(&config)?;
// → "{name myapp, port 8080, debug true}"

// With options
let styx = serde_styx::to_string_with_options(&config, &FormatOptions::default().inline())?;
```

## Test Coverage

| Crate | Tests |
|-------|-------|
| styx-format | 9 |
| facet-styx | 26 (20 unit + 6 doc) |
| serde_styx | 22 (17 unit + 5 doc) |
| styx-compat-tests | 14 |
| **Total** | **71** |

### Cross-Compatibility Tests

The `styx-compat-tests` crate verifies:
- Both libraries produce identical output for the same data
- Output from facet-styx can be parsed by serde_styx
- Output from serde_styx can be parsed by facet-styx
- Round-trips work across both libraries
- Edge cases (quoted strings, special chars) work identically

## Shared Components in styx-format

| Component | Description |
|-----------|-------------|
| `FormatOptions` | Formatting configuration (indent, max_width, thresholds) |
| `StyxWriter` | Low-level output builder with formatting |
| `can_be_bare()` | Scalar quoting decision logic |
| `escape_quoted()` | Escape handling for quoted strings |
| `unescape_quoted()` | Unescape quoted string content |
| `count_escapes()` | Heuristic for raw vs quoted |
| `count_newlines()` | Heuristic for heredoc |

## Dependencies

```toml
# styx-format/Cargo.toml
[dependencies]
styx-parse = { path = "../styx-parse" }

# facet-styx/Cargo.toml  
[dependencies]
styx-format = { path = "../styx-format" }
styx-parse = { path = "../styx-parse" }
facet-core = "0.42"
facet-format = "0.42"
facet-reflect = "0.42"

# serde_styx/Cargo.toml
[dependencies]
serde = "1.0"
styx-format = { path = "../styx-format" }
styx-parse = { path = "../styx-parse" }
```
