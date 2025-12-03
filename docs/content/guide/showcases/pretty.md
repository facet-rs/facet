+++
title = "Pretty Printing"
weight = 6
insert_anchor_links = "heading"
+++

[`facet-pretty`](https://docs.rs/facet-pretty) provides colorful, human-readable output for any `Facet` type. It's useful for debugging, logging, and REPL-style interfaces.

## Basic usage

The simplest way to pretty-print is with the `FacetPretty` trait:

```rust
use facet::Facet;
use facet_pretty::FacetPretty;

#[derive(Facet)]
struct User {
    name: String,
    email: String,
    age: u32,
}

let user = User {
    name: "Alice".into(),
    email: "alice@example.com".into(),
    age: 30,
};

println!("{}", user.pretty());
```

Output (with colors in a terminal):

```
User {
  name: "Alice",
  email: "alice@example.com",
  age: 30,
}
```

## PrettyPrinter configuration

For more control, use `PrettyPrinter` directly:

```rust
use facet_pretty::PrettyPrinter;

let printer = PrettyPrinter::new()
    .with_indent_size(4)        // Spaces per indent level (default: 2)
    .with_max_depth(3)          // Limit nesting depth
    .with_colors(false)         // Disable ANSI colors
    .with_minimal_option_names(true)  // Show Some(x) instead of Option<T>::Some(x)
    .with_doc_comments(true);   // Include doc comments in output

let output = printer.format(&user);
println!("{}", output);
```

### Available options

| Method | Description | Default |
|--------|-------------|---------|
| `with_indent_size(n)` | Spaces per indent level | 2 |
| `with_max_depth(n)` | Maximum nesting depth before truncation | unlimited |
| `with_colors(bool)` | Enable/disable ANSI color codes | `true` (unless `NO_COLOR` set) |
| `with_minimal_option_names(bool)` | Show `Some(x)` instead of `Option<T>::Some(x)` | `false` |
| `with_doc_comments(bool)` | Include `///` doc comments in output | `false` |
| `with_color_generator(gen)` | Custom color generation strategy | default HSL-based |

## Using with custom printer

Use `pretty_with()` to pass a custom printer configuration:

```rust
use facet_pretty::{FacetPretty, PrettyPrinter};

let printer = PrettyPrinter::new()
    .with_colors(false)
    .with_indent_size(4);

println!("{}", user.pretty_with(printer));
```

## Doc comment display

When `with_doc_comments(true)` is set, field documentation appears in the output:

```rust
#[derive(Facet)]
struct Config {
    /// The server hostname
    host: String,
    /// Port to listen on (1-65535)
    port: u16,
}

let config = Config {
    host: "localhost".into(),
    port: 8080,
};

let printer = PrettyPrinter::new().with_doc_comments(true);
println!("{}", printer.format(&config));
```

Output:

```
Config {
  /// The server hostname
  host: "localhost",
  /// Port to listen on (1-65535)
  port: 8080,
}
```

## Sensitive field redaction

Fields marked with `#[facet(sensitive)]` are automatically redacted:

```rust
#[derive(Facet)]
struct Credentials {
    username: String,
    #[facet(sensitive)]
    password: String,
    #[facet(sensitive)]
    api_key: String,
}

let creds = Credentials {
    username: "admin".into(),
    password: "hunter2".into(),
    api_key: "sk-1234567890".into(),
};

println!("{}", creds.pretty());
```

Output:

```
Credentials {
  username: "admin",
  password: [REDACTED],
  api_key: [REDACTED],
}
```

This works automatically — no configuration needed. It's safe to log `Facet` types without accidentally exposing secrets.

## Nested types and collections

Pretty printing handles complex nested structures:

```rust
#[derive(Facet)]
struct Team {
    name: String,
    members: Vec<User>,
    metadata: HashMap<String, String>,
}

let team = Team {
    name: "Engineering".into(),
    members: vec![
        User { name: "Alice".into(), email: "alice@example.com".into(), age: 30 },
        User { name: "Bob".into(), email: "bob@example.com".into(), age: 25 },
    ],
    metadata: [("dept".into(), "eng".into())].into_iter().collect(),
};

println!("{}", team.pretty());
```

Output:

```
Team {
  name: "Engineering",
  members: Vec<User> [
    User {
      name: "Alice",
      email: "alice@example.com",
      age: 30,
    },
    User {
      name: "Bob",
      email: "bob@example.com",
      age: 25,
    },
  ],
  metadata: HashMap<String, String> [
    "dept" => "eng",
  ],
}
```

## Byte arrays

`Vec<u8>` and `[u8; N]` are displayed as hex dumps:

```rust
#[derive(Facet)]
struct Packet {
    header: [u8; 4],
    payload: Vec<u8>,
}

let packet = Packet {
    header: [0x01, 0x02, 0x03, 0x04],
    payload: vec![0xDE, 0xAD, 0xBE, 0xEF],
};

println!("{}", packet.pretty());
```

Output:

```
Packet {
  header: [u8; 4] [ 01 02 03 04 ],
  payload: Vec<u8> [ de ad be ef ],
}
```

## Enums

Enum variants are printed with their full path:

```rust
#[derive(Facet)]
#[repr(u8)]
enum Status {
    Pending,
    Active { since: String },
    Completed(u32),
}

let statuses = vec![
    Status::Pending,
    Status::Active { since: "2024-01-01".into() },
    Status::Completed(42),
];

for s in &statuses {
    println!("{}", s.pretty());
}
```

Output:

```
Status::Pending
Status::Active {
  since: "2024-01-01",
}
Status::Completed(42,)
```

## Option display

By default, Options show their full type name:

```rust
let opt: Option<u32> = Some(42);
println!("{}", opt.pretty());
// Output: Option<u32>::Some(42)

let none: Option<u32> = None;
println!("{}", none.pretty());
// Output: Option<u32>::None
```

With `with_minimal_option_names(true)`:

```rust
let printer = PrettyPrinter::new().with_minimal_option_names(true);
println!("{}", printer.format(&Some(42u32)));
// Output: Some(42)

println!("{}", printer.format(&None::<u32>));
// Output: None
```

## Custom color generator

The `ColorGenerator` controls how colors are assigned to values:

```rust
use facet_pretty::{PrettyPrinter, ColorGenerator};

let color_gen = ColorGenerator::new()
    .with_base_hue(180.0)      // Cyan-ish base (0-360)
    .with_saturation(0.8)      // High saturation (0.0-1.0)
    .with_lightness(0.5);      // Medium brightness (0.0-1.0)

let printer = PrettyPrinter::new()
    .with_color_generator(color_gen);

println!("{}", printer.format(&user));
```

Colors are generated deterministically based on type — the same type always gets the same color, making it easy to visually scan output.

## Tokyo night color palette

The default color scheme uses the [Tokyo Night](https://github.com/tokyo-night/tokyo-night-vscode-theme) palette:

| Element | Color | RGB |
|---------|-------|-----|
| Type names | Blue | `#7aa2f7` |
| Field names | Green/Teal | `#73daca` |
| String literals | Dark green | `#9ece6a` |
| Number literals | Orange | `#ff9e64` |
| Keywords (null, true, false) | Magenta | `#bb9af7` |
| Redacted values | Red (bold) | `#db4b4b` |
| Comments | Muted gray | `#565f89` |

## NO_COLOR support

`facet-pretty` respects the `NO_COLOR` environment variable. When set, ANSI color codes are disabled automatically:

```bash
NO_COLOR=1 cargo run
```

You can also explicitly disable colors:

```rust
let printer = PrettyPrinter::new().with_colors(false);
```

## Cycle detection

Recursive structures are handled gracefully:

```rust
use std::rc::Rc;
use std::cell::RefCell;

#[derive(Facet)]
struct Node {
    value: i32,
    next: Option<Rc<RefCell<Node>>>,
}

// Even with cycles, pretty-printing won't infinite loop
// It detects the cycle and shows: /* cycle detected */
```

## Integration with logging

`facet-pretty` output works well with logging frameworks:

```rust
use log::debug;

debug!("Processing user:\n{}", user.pretty());
```

For structured logging, consider using `facet-json` instead:

```rust
use log::debug;

debug!("user={}", facet_json::to_string(&user)?);
```

## Next steps

- See [Assertions](@/guide/showcases/assert.md) for structural diffing (which uses pretty-printing internally)
- Check the [sensitive attribute](@/reference/attributes/#sensitive) for more on redaction
- Read about [facet-value](@/guide/dynamic-values.md) for dynamic type inspection
