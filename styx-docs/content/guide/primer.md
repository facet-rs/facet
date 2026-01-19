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

```json
{"name": "Alice", "age": 30}
```

In Styx:

```styx
{name Alice, age 30}
```

<div data-quiz="basics-json-to-styx"></div>

For multiline, use newlines instead of commas:

```styx
name Alice
age 30
```

Note that a STYX document is implicitly an object, so the above
is equivalent to:

```styx
{
    name Alice
    age 30
}
```

<div data-quiz="basics-when-quotes"></div>

### Sequences

Arrays are called sequences. They use parentheses:

```styx
colors (red green blue)
```

<div data-quiz="basics-sequence-syntax"></div>

<div data-quiz="basics-comma-sequence"></div>

### Separator rules

Objects use either commas OR newlines - never both.

<div data-quiz="basics-mixing-separators"></div>

<div data-quiz="basics-duplicate-keys"></div>

## Scalars are just text

Here's something different from JSON: values don't have types.

```styx
name Alice
age 30
active true
```

Is `30` a number? Is `true` a boolean? In Styx: **no**. They're all just text atoms.

<div data-quiz="scalars-number"></div>

This solves the Norway problem:

<div data-quiz="scalars-norway"></div>

Types come from schemas or deserialization - not from the syntax. This means URLs, paths, and other complex values just work:

<div data-quiz="scalars-url"></div>

## The two dimensions

Every Styx value has two parts:
- A **tag** (what kind of thing)
- A **payload** (the thing itself)

<div data-quiz="tags-two-dimensions"></div>

When you write `@rgb(255 128 0)`, you're explicitly setting both:

<div data-quiz="tags-explicit"></div>

When there's no explicit payload, it defaults to `@` (unit):

<div data-quiz="tags-unit-payload"></div>

### The space rule

This is the most important rule in Styx:

**There is never a space between a tag and its payload.**

```styx
@tag()    // tag=tag, payload=()
@tag ()   // TWO atoms: @tag and ()
```

<div data-quiz="tags-space-matters"></div>

<div data-quiz="tags-three-atoms"></div>

Tags work with any payload type:

<div data-quiz="tags-on-objects"></div>

## Unit and elision

`@` by itself is the **unit** value - like `null` but more principled.

<div data-quiz="unit-what-is"></div>

### Canonical vs idiomatic

Styx has a fully explicit (canonical) form and a shorter (idiomatic) form:

| Canonical | Idiomatic | Meaning |
|-----------|-----------|---------|
| `@@` | `@` | unit value |
| `@ok@` | `@ok` | tag `ok`, payload unit |
| `key @` | `key` | key with unit value |

<div data-quiz="unit-canonical"></div>

<div data-quiz="unit-elision-key"></div>

Unit is a value like any other:

<div data-quiz="unit-sparse-sequence"></div>

## Key chains and attributes

Multiple bare words form nested objects:

```styx
database connection timeout 30
```

Expands to:

```styx
database {connection {timeout 30}}
```

<div data-quiz="keychains-basic"></div>

### Object attributes

For tabular data, `key>value` syntax is cleaner:

```styx
{
    web  host>example.org   port>80
    api  host>api.example   port>8080
}
```

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

<div data-quiz="heredoc-basic"></div>

The language hint after the marker enables syntax highlighting:

```styx
code <<SRC,rust
fn main() {
    println!("Hello!");
}
SRC
```

<div data-quiz="heredoc-hint"></div>

## Schemas

Styx documents can have schemas - also written in Styx:

```styx
schema {
    @ @object{
        name @string
        age @int
        tags @seq(@string)
    }
}
```

Doc comments (`///`) attach documentation to elements:

```styx
schema {
    @ @object{
        /// User's display name
        name @string
        /// Age in years
        age @int
    }
}
```

<div data-quiz="schema-doc-comments"></div>

<div data-quiz="schema-types"></div>

<div data-quiz="schema-validation"></div>

## That's it!

You now know Styx. The key points:

1. **Clean syntax**: no colons, minimal quotes, whitespace-separated
2. **Untyped scalars**: types come from schemas, not syntax
3. **Tags**: `@name` attaches meaning, no space before payload
4. **Unit**: `@` is null-like, can be elided
5. **Schemas**: validation anywhere, editor support everywhere

<script type="module" src="/src/quiz/main.ts"></script>
