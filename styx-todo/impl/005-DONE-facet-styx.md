# Phase 005: facet-styx

Deserializer and serializer for Styx using the facet reflection system.

## Deliverables

- `crates/facet-styx/src/lib.rs` - Crate root
- `crates/facet-styx/src/de.rs` - Deserializer
- `crates/facet-styx/src/ser.rs` - Serializer  
- `crates/facet-styx/src/error.rs` - Error types

## Dependencies

```toml
[dependencies]
facet = "..."
styx-parse = { path = "../styx-parse" }
```

Note: facet-styx consumes events directly from styx-parse, NOT from styx-tree. This allows streaming deserialization without building an intermediate tree.

## Deserializer Design

```rust
use facet::Deserialize;

pub struct Deserializer<'src> {
    parser: Parser<'src>,
    // event buffer for lookahead
    // current position state
}

impl<'src> Deserializer<'src> {
    pub fn new(source: &'src str) -> Self;
}
```

### Type Mapping

| Styx | Rust |
|------|------|
| Scalar | `String`, `&str`, integers, floats, bool (parsed) |
| Unit `@` | `()`, `Option::None` |
| Sequence `(a b c)` | `Vec<T>`, `[T; N]`, tuples |
| Object `{k v}` | structs, `HashMap<K, V>` |
| Tagged `@variant{...}` | enums |

### Scalar Parsing

Scalars are opaque text until deserialization. The deserializer parses them based on target type:

```rust
// "42" as Scalar
let n: i32 = from_str("42")?;        // parses to 42
let s: String = from_str("42")?;     // keeps as "42"
let b: bool = from_str("true")?;     // parses to true
```

### Enum Deserialization

Enums use tags:

```styx
// Unit variant
@none

// Newtype variant  
@some 42

// Struct variant
@error { code 500, message "oops" }

// Tuple variant (sequence payload)
@point(10 20)
```

```rust
enum MyEnum {
    None,
    Some(i32),
    Error { code: i32, message: String },
    Point(i32, i32),
}
```

### Optional Fields

```rust
struct Config {
    required: String,
    optional: Option<i32>,  // can be absent or @
}
```

```styx
{
    required "hello"
    // optional is absent → None
}

// or
{
    required "hello"
    optional @  // explicit unit → None
}

// or
{
    required "hello"
    optional 42  // Some(42)
}
```

### Flattening / Nested Keys

The parser emits nested key paths. The deserializer handles them:

```styx
server host localhost
server port 8080
```

Becomes logically:
```styx
server {
    host localhost
    port 8080
}
```

## Serializer Design

```rust
pub fn to_string<T: Serialize>(value: &T) -> Result<String, Error>;

pub fn to_string_formatted<T: Serialize>(
    value: &T, 
    options: &FormatOptions
) -> Result<String, Error>;
```

### Formatting Options

```rust
pub struct FormatOptions {
    pub indent: String,
    pub use_comma_separator: bool,
    pub inline_threshold: usize,  // inline objects smaller than this
}
```

## API

```rust
// Deserialize from string
pub fn from_str<'de, T: Deserialize<'de>>(s: &'de str) -> Result<T, Error>;

// Serialize to string
pub fn to_string<T: Serialize>(value: &T) -> Result<String, Error>;

// With options
pub fn to_string_pretty<T: Serialize>(value: &T) -> Result<String, Error>;
```

## Error Messages

Errors should include:
- Source location (line, column)
- Expected vs found
- Context (what were we trying to deserialize)

```
error: expected integer
 --> config.styx:5:12
  |
5 |     port "not a number"
  |          ^^^^^^^^^^^^^^ found string
  |
  = while deserializing field `port` of struct `ServerConfig`
```

## Testing

- Deserialize all primitive types
- Deserialize structs, enums, collections
- Serialize and round-trip
- Error message quality tests
- Compatibility with facet derive macros
- Compare with facet-json behavior where applicable
