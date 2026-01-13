# Phase 005a: Serialization Rules

When serializing Rust values to Styx, there are multiple valid representations. This document specifies the default choices and available options.

## Decision Points

| Choice | Options | Default |
|--------|---------|---------|
| Object separator | newline vs comma | **heuristic** (see below) |
| Scalar quoting | bare vs quoted vs raw | bare when valid |
| Object nesting | flat key-paths vs nested braces | nested braces |
| Sequence layout | single-line vs multi-line | heuristic |
| Attributes syntax | `k=v` vs `{k v}` | never use attributes |
| Unit representation | `@` vs omit field | `@` for explicit None |

## Scalar Representation

### Rule: Prefer Bare Scalars

Use bare scalars when the value:
1. Contains no special characters: `{}(),"=@` or whitespace
2. Is not empty
3. Does not start with `//` (would look like comment)
4. Does not start with `r#` or `<<` (would look like raw/heredoc)

Otherwise, use quoted scalars.

```rust
// Bare
"localhost" → localhost
"8080" → 8080
"true" → true
"hello-world" → hello-world
"https://example.com/path" → https://example.com/path

// Must quote
"hello world" → "hello world"
"" → ""
"say \"hi\"" → "say \"hi\""
"with\nnewline" → "with\nnewline"
"{braces}" → "{braces}"
```

### Rule: Use Raw Strings for Complex Escaping

If a string would require many escapes, prefer raw strings:

```rust
// Many escapes → raw string
r#"C:\Users\name"# → r#"C:\Users\name"#
```

Heuristic: if quoted form has > 3 escapes, consider raw.

### Rule: Use Heredocs for Multi-line Text

Multi-line strings (containing `\n`) with > 1 line use heredoc:

```rust
"line1\nline2\nline3" →
<<TEXT
line1
line2
line3
TEXT
```

Single embedded newline can stay quoted: `"line1\nline2"`

## Object Representation

### Separator Heuristic

The serializer chooses between comma (inline) and newline (multi-line) based on:

```
USE COMMA (inline) when ALL of:
  ├─ Total rendered width ≤ available_width
  ├─ Entry count ≤ inline_threshold (default 4)
  ├─ No nested objects or sequences
  ├─ No heredocs or multi-line scalars
  ├─ No doc comments on entries
  └─ Not the document root (root always uses newlines)

USE NEWLINE (multi-line) otherwise
```

### Depth-Aware Width

The available width shrinks with nesting depth:

```
available_width = max_width - (depth * indent_width)
```

Example with `max_width=80` and `indent="    "` (4 spaces):

| Depth | Available Width |
|-------|-----------------|
| 0 (root) | 80 (but root always multi-line) |
| 1 | 76 |
| 2 | 72 |
| 3 | 68 |
| 4 | 64 |
| 5 | 60 |
| ... | ... |
| 10 | 40 |

At some point (e.g., available_width < 30), force multi-line regardless of content, since inline becomes unreadable.

### Examples

**Inline (comma):**
```styx
{host localhost, port 8080}
{enabled true, retries 3, timeout 30}
{x 0, y 0, z 0}
```

**Multi-line (newline):**
```styx
{
    host localhost
    port 8080
    tls {cert /path/to/cert, key /path/to/key}
}
```

Note: nested `tls` object is itself inline because it's simple enough.

**Always multi-line:**
```styx
// Document root
{
    server {host localhost, port 8080}
    database {host db.local, port 5432}
}

// Has doc comments
{
    /// The server hostname
    host localhost
    /// The server port  
    port 8080
}

// Has heredoc
{
    name my-script
    content <<BASH
        echo "hello"
        BASH
}
```

### Depth-Aware Formatting

The heuristic is applied recursively. A deeply nested object might inline even if its parent doesn't:

```styx
{
    metadata {name my-app, namespace default}
    spec {
        replicas 3
        selector {matchLabels {app my-app}}
        template {
            metadata {labels {app my-app}}
            spec {
                containers (
                    {name app, image nginx:latest, ports ({containerPort 80})}
                )
            }
        }
    }
}
```

### Width Calculation

To decide if an object fits inline, calculate:

```
available_width = max_width - (depth * indent_width)

object_width = 2 (braces) 
             + sum(entry_widths) 
             + 2 * (entry_count - 1)  // ", " separators

entry_width = key_width + 1 (space) + value_width
```

Decision:
```
if available_width < min_inline_width (default 30):
    USE NEWLINE  // too cramped for inline
else if object_width > available_width:
    USE NEWLINE
else:
    USE COMMA
```

### Rule: Never Flatten to Key-Paths

Always use explicit nesting, never key-path shorthand:

```styx
// Always this:
server {
    host localhost
    port 8080
}

// Never this (even though valid):
server host localhost
server port 8080
```

Rationale: explicit structure is clearer, key-paths are a parsing convenience not a serialization target.

### Rule: Never Use Attributes Syntax

Attributes (`k=v`) are a parsing convenience. Serializer always uses object syntax.

```styx
// Never emit this:
spec selector matchLabels app=web

// Always this:
spec {
    selector {
        matchLabels {
            app web
        }
    }
}
```

## Sequence Representation

### Sequence Heuristic

Similar to objects:

```
USE SINGLE-LINE when ALL of:
  ├─ Total rendered width ≤ max_width
  ├─ Item count ≤ inline_threshold (default 8 for sequences)
  ├─ All items are scalars or small inline objects
  └─ No heredocs or multi-line content

USE MULTI-LINE otherwise
```

### Examples

**Single-line:**
```styx
(1 2 3 4 5)
(localhost 127.0.0.1 ::1)
(alice bob charlie)
({x 0, y 0} {x 1, y 1} {x 2, y 2})
```

**Multi-line:**
```styx
(
    {name alice, age 30}
    {name bob, age 25}
    {name charlie, age 35}
)

(
    /usr/local/bin
    /usr/bin
    /bin
    /usr/sbin
    /sbin
    /opt/homebrew/bin
    /home/user/.local/bin
    /home/user/go/bin
    /home/user/.cargo/bin
)
```

## Enum Representation

### Rule: Tag Syntax for All Variants

```rust
enum Status {
    Ok,
    Error(String),
    Complex { code: i32, msg: String },
}
```

```styx
@ok
@error "message"
@complex {code 500, msg "oops"}
```

### Rule: Unit Tags Without Explicit `@`

`@ok` not `@ok@`

## Optional Fields

### Rule: Omit None by Default

```rust
struct Config {
    name: String,
    description: Option<String>,
}

Config { name: "foo".into(), description: None }
```

```styx
{
    name foo
    // description omitted
}
```

### Option: Explicit None

With serializer option `emit_none: true`:

```styx
{
    name foo
    description @
}
```

## Formatting Options

```rust
pub struct SerializeOptions {
    /// Indentation string (default: "    " - 4 spaces)
    pub indent: String,
    
    /// Max line width before wrapping (default: 80)
    pub max_width: usize,
    
    /// Minimum available width to even consider inline (default: 30)
    /// If depth eats into max_width below this, force multi-line
    pub min_inline_width: usize,
    
    /// Inline objects with ≤ N entries (default: 4)
    pub inline_object_threshold: usize,
    
    /// Inline sequences with ≤ N items (default: 8)  
    pub inline_sequence_threshold: usize,
    
    /// Use heredocs for strings with > N lines (default: 2)
    pub heredoc_line_threshold: usize,
    
    /// Emit `@` for None values (default: false)
    pub emit_none: bool,
    
    /// Force all objects to use newline separators (default: false)
    pub force_multiline: bool,
    
    /// Force all objects to use comma separators (default: false)
    /// Takes precedence over force_multiline if both set
    pub force_inline: bool,
}

impl Default for SerializeOptions {
    fn default() -> Self {
        Self {
            indent: "    ".into(),
            max_width: 80,
            min_inline_width: 30,
            inline_object_threshold: 4,
            inline_sequence_threshold: 8,
            heredoc_line_threshold: 2,
            emit_none: false,
            force_multiline: false,
            force_inline: false,
        }
    }
}
```

## Compact Mode

For embedding in other formats or single-line output:

```rust
pub fn to_string_compact<T: Serialize>(value: &T) -> String;
```

Rules:
- Always use comma separators
- Always inline (no newlines)
- Minimal whitespace

```styx
{server {host localhost, port 8080}, enabled true}
```

## Examples

### Kubernetes-like Config

```rust
struct Deployment {
    api_version: String,
    kind: String,
    metadata: Metadata,
    spec: DeploymentSpec,
}
```

Serializes to:

```styx
{
    api_version apps/v1
    kind Deployment
    metadata {
        name my-app
        namespace default
    }
    spec {
        replicas 3
        selector {
            match_labels {
                app my-app
            }
        }
        template {
            // ...
        }
    }
}
```

### Simple Config

```rust
struct Config {
    debug: bool,
    port: u16,
    hosts: Vec<String>,
}
```

Serializes to:

```styx
{
    debug true
    port 8080
    hosts (localhost 127.0.0.1 ::1)
}
```
