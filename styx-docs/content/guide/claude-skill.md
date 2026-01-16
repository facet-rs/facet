+++
title = "Claude Code Skill"
weight = 10
slug = "claude-skill"
insert_anchor_links = "heading"
+++

A skill for [Claude Code](https://docs.anthropic.com/en/docs/claude-code) that teaches it Styx syntax and schemas.

## Installation

### Option 1: From the CLI

```bash
mkdir -p ~/.claude/skills/styx
styx @skill > ~/.claude/skills/styx/SKILL.md
```

### Option 2: One-liner from GitHub

```bash
mkdir -p ~/.claude/skills/styx
curl -sL https://raw.githubusercontent.com/bearcove/styx/main/contrib/claude-skill/SKILL.md \
  > ~/.claude/skills/styx/SKILL.md
```

### Option 3: Manual copy

Clone the repo and copy `contrib/claude-skill/SKILL.md` to `~/.claude/skills/styx/`.

## Usage

Once installed, Claude Code automatically has access to Styx knowledge when working in projects with `.styx` files.

You can also explicitly invoke the skill:

```
/styx
```

## What it teaches

The skill covers:

- **Syntax** — scalars (bare, quoted, raw, heredoc), objects, sequences, tags
- **Schema language** — type constraints, optional fields, enums, named types
- **CLI usage** — validation, formatting, JSON conversion
- **Common mistakes** — separator mixing, boolean syntax, sequence commas

## Skill content

The full skill is reproduced below for reference. This is what gets loaded into Claude's context.

---

````markdown
---
name: styx
description: Styx configuration language syntax and schema reference. Use when writing or editing .styx files, creating schemas, or debugging syntax errors. Covers syntax (scalars, objects, sequences, tags, heredocs), schema language, and CLI validation.
---

# Styx Configuration Language

Styx is a configuration language with schema support, comments, and flexible syntax.

## Quick Reference

```styx
// Line comment
/// Doc comment (attaches to next entry)

// Schema declaration (optional, enables validation)
@ path/to/schema.styx

// Key-value pairs
name "My Config"
port 8080
enabled @true

// Nested objects (newline or comma separated)
server {
    host localhost
    port 8080
}
limits {max 100, timeout 30}

// Sequences
hosts (localhost "127.0.0.1" server.example.com)

// Tags for enums/variants
status @ok
level @warn
error @error{code 500, message "fail"}

// Heredocs for multi-line content
query <<SQL
SELECT * FROM users
WHERE active = true
SQL
```

## Scalars (Values)

Four forms of string values:

| Form | Example | Use |
|------|---------|-----|
| Bare | `localhost`, `8080`, `my-value` | Simple values without special chars |
| Quoted | `"hello world"`, `"with\nnewline"` | Strings with spaces/escapes |
| Raw | `r"C:\path"`, `r#"has "quotes""#` | No escape processing |
| Heredoc | `<<TAG`...`TAG` | Multi-line content |

**Bare scalars**: Any characters except `{}(),"=@ \t\n\r`. Cannot start with `@`.

**Quoted strings**: Escapes: `\\`, `\"`, `\n`, `\r`, `\t`, `\u{hex}`.

**Raw strings**: `r"..."` or `r#"..."#` (add `#` to include quotes).

**Heredocs**:
```styx
content <<DELIM
Multi-line text here.
Preserves all whitespace.
DELIM

// With language hint (for syntax highlighting)
code <<CODE,rust
fn main() {
    println!("Hello!");
}
CODE
```

## Objects

Objects use `{` `}` with either newline or comma separation:

```styx
// Newline separated
server {
    host localhost
    port 8080
}

// Comma separated (single line)
server {host localhost, port 8080}

// Cannot mix separators in same block
```

## Sequences

Sequences use `(` `)` with whitespace separation:

```styx
// Simple sequence
ports (80 443 8080)

// With quoted strings
hosts ("localhost" "example.com")

// Nested sequences
matrix ((1 2 3) (4 5 6))

// Sequence of objects
routes (
    @route{path "/", handler index}
    @route{path "/api", handler api}
)
```

## Tags

Tags start with `@` and optionally have a payload:

```styx
// Unit tag (no payload) - shorthand for @true@
enabled @true

// Tag with object payload
error @error{code 500, message "Internal error"}

// Tag with sequence payload
point @rgb(255 128 0)

// Tag with string payload
message @warn"Deprecated feature"

// Bare @ is the unit value
nothing @
```

**Common built-in tags**: `@true`, `@false`, `@none`, `@some(...)`.

## Unit Value

The bare `@` is the unit value (like null/none):

```styx
optional_field @
```

## Attributes

HTML-style `key=value` attributes (no space around `=`):

```styx
div id=main class="container" data-count=42
```

## Comments

```styx
// Single-line comment

/// Doc comment - attaches to next entry
/// Can span multiple lines
name "documented field"
```

## Schema Declaration

Documents can declare their schema for validation:

```styx
// External schema file
@ ./my-schema.styx

// Or inline schema
@ {
    schema {
        @ @object{name @string, port @int}
    }
}
```

## Schema Language

Schema files define expected structure:

```styx
meta {
    id https://example.com/my-schema
    version 2026-01-16
    description "My configuration schema"
}

schema {
    // @ defines the document root structure
    @ @object{
        /// Server display name
        name @string
        /// Port number (1-65535)
        port @int{min 1, max 65535}
        /// Enable debug mode
        debug @optional(@bool)
        /// Default timeout in ms
        timeout @default(30000 @int)
    }
}
```

### Type Tags

| Tag | Description |
|-----|-------------|
| `@string` | Any scalar value |
| `@int` | Integer with optional `{min N, max N}` |
| `@float` | Float with optional `{min N, max N}` |
| `@bool` | `@true` or `@false` |
| `@unit` | The unit value `@` |
| `@any` | Any value |
| `@optional(@T)` | Field may be absent |
| `@default(val @T)` | Default value if absent |
| `@seq(@T)` | Sequence where all elements match `@T` |
| `@object{...}` | Object with named fields |
| `@union(@A @B)` | Value matches any listed type |
| `@enum{...}` | Tagged variants |
| `@map(@V)` | String keys to `@V` values |
| `@map(@K @V)` | `@K` keys to `@V` values |
| `@flatten(@Type)` | Inline fields from another type |
| `@deprecated("msg" @T)` | Deprecated field (warning) |

### String Constraints

```styx
username @string{minLen 3, maxLen 20}
slug @string{pattern "^[a-z0-9-]+$"}
```

### Named Types

Define reusable types with PascalCase names:

```styx
schema {
    @ @object{
        server @Server
        logging @optional(@Logging)
    }

    Server @object{
        host @string
        port @int{min 1, max 65535}
    }

    Logging @object{
        level @enum{debug, info, warn, error}
        output @optional(@string)
    }
}
```

### Enums

Define variants with optional payloads:

```styx
schema {
    status @enum{
        ok
        pending
        error @object{message @string, code @int}
    }
}
```

Usage in document:
```styx
status @ok
status @pending
status @error{message "Failed", code 500}
```

### Open Objects

By default objects are closed (unknown keys forbidden). Use `@` key for open objects:

```styx
// Allow any additional string fields
Labels @object{
    @ @string
}

// Known fields plus extras
Config @object{
    name @string
    @ @string
}
```

## CLI Usage

```bash
# Format and print to stdout
styx config.styx

# Format in place
styx config.styx --in-place

# Validate against declared schema
styx config.styx --validate

# Convert to JSON
styx config.styx --json-out -

# Use different schema
styx config.styx --validate --override-schema other.schema.styx

# Show parse tree (debugging)
styx @tree config.styx

# Start language server
styx @lsp
```

## Common Mistakes

**Wrong**: Mixing separators in objects
```styx
// WRONG - can't mix newlines and commas
server {
    host localhost, port 8080
}
```
**Right**: Pick one separator style per block.

**Wrong**: Space around `=` in attributes
```styx
// WRONG
div id = main
```
**Right**: No spaces: `div id=main`

**Wrong**: Using `null` or `true`/`false` as keywords
```styx
// WRONG - these are bare scalars, not booleans
enabled true
value null
```
**Right**: Use tags for booleans and unit:
```styx
enabled @true
value @none
nothing @
```

**Wrong**: Comma in sequences
```styx
// WRONG - sequences are whitespace-separated
ports (80, 443, 8080)
```
**Right**: Whitespace only:
```styx
ports (80 443 8080)
```

## File Extensions

- `.styx` - Styx data files
- `.schema.styx` - Styx schema files (convention)

## Editor Support

- **Zed**: Built-in support via tree-sitter grammar
- **VS Code**: Extension available
- **LSP**: Run `styx @lsp` for any editor with LSP support
````
