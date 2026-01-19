+++
title = "Primer"
weight = 1
slug = "primer"
insert_anchor_links = "heading"
+++

Styx is a configuration language. Compared to JSON:

- **Bare scalars** — quotes only when necessary
- **Whitespace separation** — no `:` between keys and values
- **Flexible separators** — commas or newlines, not both
- **Scalars are opaque** — the parser assigns no type information
- **Two-dimensional values** — every value has a tag and a payload

## The basics

A JSON object like this:

```json
{"name": "Alice", "age": 30}
```

Becomes this in Styx:

```styx
{name Alice, age 30}
```

The differences:

- No colons between keys and values — whitespace separates them
- No quotes around `Alice` — bare scalars work for simple text
- `30` is not a number, just an atom (more on this later)

### When you need quotes

A bare scalar is terminated by whitespace or any of: `{`, `}`, `(`, `)`, `,`, `"`, `>`.

A bare scalar cannot *start* with `@` or `=`, but these are fine after the first character. This allows URLs with `@` and query strings with `=`.

So paths and URLs work unquoted:

```styx
path /usr/local/bin
url https://example.com/path?query=value
```

But you need quotes when your value contains terminating characters:

```styx
greeting "Hello, world!"
template "{{name}}"
```

For values with many quotes or escapes, use raw scalars (`r#"..."#`) or heredocs:

```styx
// Raw scalar - no escape processing
json r#"{"key": "value"}"#

// Heredoc - for multiline content
script <<SH
echo "Hello, $USER"
SH
```

### Multiline objects

For multiline, use newlines instead of commas:

```styx
name Alice
age 30
active true
```

A Styx document is implicitly an object, so the above is equivalent to:

```styx
{
    name Alice
    age 30
    active true
}
```

Objects use either commas OR newlines as separators — never both in the same object. This prevents the ambiguity that plagues YAML.

### Sequences

Ordered collections use parentheses and are called sequences:

```styx
colors (red green blue)
```

Sequences are whitespace-separated. Commas are not allowed:

```styx
// WRONG: commas in sequences
colors (red, green, blue)

// RIGHT: whitespace-separated
colors (red green blue)
```

### Recap

<div data-quiz="basics-json-to-styx"></div>

<div data-quiz="basics-when-quotes"></div>

<div data-quiz="basics-sequence-syntax"></div>

<div data-quiz="basics-comma-sequence"></div>

<div data-quiz="basics-mixing-separators"></div>

## Scalars are opaque text

Here's a fundamental difference from JSON: the parser assigns no type information to scalars.

```styx
name Alice
age 30
active true
ratio 3.14
```

In JSON, `30` is a number, `true` is a boolean, and `3.14` is a float. In Styx, they're all just text. The parser doesn't distinguish between them.

This might seem limiting, but it solves real problems:

**The Norway problem**: In YAML, `NO` (the country code for Norway) is parsed as boolean `false`. In Styx, `NO` is just the text `NO`.

**Large numbers**: JSON parsers often lose precision on large integers. In Styx, `9007199254740993` is preserved exactly as text until your application parses it.

**Version strings**: Is `1.0` a number or a version? In Styx, it doesn't matter at parse time — your schema or application decides.

Types come from schemas or deserialization, not from syntax. This means URLs, paths, and other complex values work without escaping:

```styx
url https://example.com/path?query=value
email user@example.com
version 2.0.0-beta.1
```

### Recap

<div data-quiz="scalars-number"></div>

<div data-quiz="scalars-norway"></div>

<div data-quiz="scalars-url"></div>

## The two dimensions

Every Styx value has two parts:

- A **tag** — identifies what kind of thing it is
- A **payload** — the thing itself

Both default to `@` (the unit value) when not specified.

When you write a bare scalar like `Alice`, the full form is `@"Alice"` — tag is unit, payload is the text.

When you write `@pending`, the full form is `@pending@` — tag is `pending`, payload is unit.

When you write `@rgb(255 128 0)`, you're explicitly setting both: tag is `rgb`, payload is the sequence `(255 128 0)`.

### Tags in practice

Tags are useful for:

**Discriminated unions** — distinguishing between variants:

```styx
result @ok{data "success"}
result @err{message "not found"}
```

**Type hints** — indicating how to interpret a value:

```styx
created @datetime"2024-01-15T10:30:00Z"
color @hex"#ff5500"
```

**Nullability** — distinguishing "no value" from "empty":

```styx
middle_name @none
nickname ""
```

### The space rule

This is the most important whitespace rule in Styx:

**There is never a space between a tag and its payload.**

```styx
@tag()     // ONE value: tag=tag, payload=()
@tag ()    // TWO values: @tag and ()
```

This matters because `@tag()` is a single value (a tagged empty sequence), while `@tag ()` is two separate values. In an entry context:

```styx
a @tag()   // key=a, value=@tag()  — valid
b @tag ()  // three atoms: b, @tag, () — invalid!
```

Entries can only have two atoms (key and value). The space between `@tag` and `()` makes them separate atoms, resulting in three atoms total.

### Recap

<div data-quiz="tags-two-dimensions"></div>

<div data-quiz="tags-explicit"></div>

<div data-quiz="tags-unit-payload"></div>

<div data-quiz="tags-space-matters"></div>

<div data-quiz="tags-standalone-vs-payload"></div>

<div data-quiz="tags-three-atoms"></div>

<div data-quiz="tags-on-objects"></div>

## Unit and elision

`@` by itself is the **unit** value. It represents the absence of a meaningful value — similar to `null` in other languages, but more principled because it's just another value, not a special case.

### Canonical vs idiomatic forms

Styx has a fully explicit (canonical) form and shorter (idiomatic) forms:

| Canonical | Idiomatic | Meaning |
|-----------|-----------|---------|
| `@@` | `@` | unit value (tag=unit, payload=unit) |
| `@ok@` | `@ok` | tag `ok` with unit payload |
| `key @` | `key` | key with unit value |

The idiomatic forms are preferred in practice. The canonical forms exist for completeness and are useful when you need to be explicit.

### Unit in sequences

Unit values can appear in sequences, which is useful for sparse data:

```styx
row (1 @ @ 4 5)  // sparse row with gaps
```

### Recap

<div data-quiz="unit-what-is"></div>

<div data-quiz="unit-canonical"></div>

<div data-quiz="unit-elision-key"></div>

<div data-quiz="unit-sparse-sequence"></div>

## Key chains

Multiple bare words in sequence form nested objects automatically:

```styx
database connection timeout 30
```

This expands to:

```styx
database {connection {timeout 30}}
```

This is purely syntactic sugar — the parser produces the same tree either way. It's useful for deeply nested configuration:

```styx
server http port 8080
server http host localhost
server tls enabled true
server tls cert "/etc/ssl/cert.pem"
```

### Attributes

For tabular data, the `key>value` attribute syntax is more readable:

```styx
{
    web  host>example.org port>80
    api  host>api.example port>8080
    db   host>localhost   port>5432
}
```

Attributes produce the same structure as nested objects but read better for record-like data.

### Recap

<div data-quiz="keychains-basic"></div>

<div data-quiz="attributes-basic"></div>

<div data-quiz="attributes-multiple"></div>

## Heredocs

Multiline strings use heredoc syntax:

```styx
query <<SQL
SELECT * FROM users
WHERE active = true
SQL
```

The delimiter must be uppercase letters (optionally with digits and underscores). The closing delimiter ends the heredoc.

### Language hints

A language hint after the delimiter enables syntax highlighting in editors:

```styx
code <<SRC,rust
fn main() {
    println!("Hello!");
}
SRC
```

The hint (`,rust`) is metadata — it doesn't affect the content.

### Indented heredocs

If the closing delimiter is indented, that indentation is stripped from all content lines:

```styx
script <<BASH
    echo "hello"
    echo "world"
    BASH
```

The content is `echo "hello"\necho "world"\n` — the leading spaces are removed.

### Recap

<div data-quiz="heredoc-basic"></div>

<div data-quiz="heredoc-hint"></div>

## Schemas

Styx documents can declare a schema for validation. Schemas are also written in Styx:

```styx
@schema{
    @ @object{
        name @string
        age @int
        tags @seq(@string)
    }
}
```

Doc comments (`///`) attach documentation to schema elements:

```styx
@schema{
    @ @object{
        /// User's display name
        name @string
        /// Age in years
        age @int
    }
}
```

Schemas enable editor features like autocomplete, hover documentation, and validation as you type.

### Recap

<div data-quiz="schema-doc-comments"></div>

<div data-quiz="schema-types"></div>

## Summary

The key concepts:

1. **Whitespace-separated syntax** — no colons, minimal quotes
2. **Opaque scalars** — types come from schemas, not syntax
3. **Two dimensions** — every value has a tag and a payload
4. **The space rule** — no space between tag and payload
5. **Unit and elision** — `@` is the unit value, often implicit
6. **Schemas** — validation and editor support

For the full specification, see the [reference documentation](/reference/).

<script type="module" src="/src/quiz/main.ts"></script>
