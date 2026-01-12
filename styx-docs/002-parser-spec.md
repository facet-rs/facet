---
weight = 2
slug = "parser-spec"
---

# Part 1: Parser

The parser converts STYX source text into a document tree.

## Document structure

A STYX document is an object. Top-level entries do not require braces.

> r[document.root]
> The parser MUST interpret top-level key-value pairs as entries of an implicit root object.
> Root entries follow the same separator rules as block objects: newlines or commas.
> If the document starts with `{`, it MUST be parsed as a single explicit block object.
> 
> ```compare
> /// styx
> // Implicit root
> server {
>   host localhost
>   port 8080
> }
> /// styx
> // Explicit root
> {
>   server {
>     host localhost
>     port 8080
>   }
> }
> ```

> r[comment.line]
> Line comments start with `//` and extend to the end of the line.
> Comments MUST be preceded by whitespace.
> 
> ```styx
> host localhost  // comment
> url https://example.com  // the :// is not a comment
> ```

## Value types

The parser produces four value types:

  * **Scalar** — an opaque text atom
  * **Sequence** — an ordered list of values: `(a b c)`
  * **Object** — an ordered map of keys to values: `{ key value }`
  * **Unit** — the absence of a meaningful value: `@`

Any value may be tagged: `@tag{ ... }`, `@tag(...)`, `@tag"..."`, `@tag@`.

## Tags

A tag is an identifier prefixed with `@` that labels a value.

> r[tag.syntax]
> A tag MUST match the pattern `@[A-Za-z_][A-Za-z0-9_-]*`.

> r[tag.payload]
> A tag MUST be immediately followed (no whitespace) by its payload:
> 
> | Follows `@tag` | Result |
> |-----------------------|--------|
> | `{...}` | tagged object |
> | `(...)` | tagged sequence |
> | `"..."`, `r#"..."#`, or `<<HEREDOC` | tagged scalar |
> | `@` | tagged unit |
> 
> ```styx
> status @ok@                // tagged unit
> result @err{message "x"}   // tagged object
> color @rgb(255 128 0)      // tagged sequence
> name @nickname"Bob"        // tagged scalar
> ```
> 
> Note: bare scalars cannot be tagged — there's no delimiter to separate tag from value.

## Scalars

Scalars are opaque text atoms. The parser assigns no meaning to them.

### Bare scalars

> r[scalar.bare.termination]
> A bare scalar is terminated by whitespace or any of: `}`, `)`, `,`, `@`.
> 
> ```styx
> url https://example.com/path?query=1
> ```

### Quoted scalars

> r[scalar.quoted.escapes]
> Quoted scalars use `"..."` and support escape sequences:
> `\\`, `\"`, `\n`, `\r`, `\t`, `\0`, `\uXXXX`, `\u{X...}`.
> 
> ```styx
> greeting "hello\nworld"
> ```

### Raw scalars

> r[scalar.raw.syntax]
> Raw scalars use `r#"..."#` syntax. The number of `#` must match.
> Content is literal — escape sequences are not processed.
> 
> ```styx
> pattern r#"no need to escape "quotes" or \n"#
> ```

### Heredoc scalars

> r[scalar.heredoc.syntax]
> Heredocs start with `<<DELIMITER` and end with the delimiter on its own line.
> The delimiter MUST match `[A-Z][A-Z0-9_]*` and not exceed 16 characters.
> Leading whitespace is stripped up to the closing delimiter's indentation.
> Content is literal — escape sequences are not processed.
> 
> ```styx
> script <<BASH
>   echo "hello"
>   BASH
> ```

## Sequences

> r[sequence.syntax]
> Sequences use `(` `)` delimiters. Elements are separated by whitespace.
> Commas are NOT allowed.
> 
> ```styx
> numbers (1 2 3)
> nested ((a b) (c d))
> ```

> r[sequence.elements]
> Elements may be scalars, objects, sequences, unit, or tagged values.

## Objects

> r[object.syntax]
> Objects use `{` `}` delimiters. Entries are `key value` pairs separated by newlines or commas (not both).
> Keys are bare or quoted scalars. Duplicate keys are forbidden.
> 
> ```styx
> server {
>   host localhost
>   port 8080
> }
> { a 1, b 2, c 3 }         // comma-separated
> { "key with spaces" 42 }  // quoted key
> ```

## Unit

> r[value.unit]
> The token `@` not followed by an identifier is the unit value.
> 
> ```styx
> enabled @
> ```

## Shorthand syntax

The following shorthand forms are equivalent to their canonical forms.

### Implicit unit

> r[shorthand.implicit-unit]
> A key without a value has implicit unit value. A tag without a payload has implicit unit payload.
> 
> ```compare
> /// styx
> // Shorthand
> enabled
> status @ok
> /// styx
> // Canonical
> enabled @
> status @ok@
> ```

### Attribute objects

> r[shorthand.attr]
> Attribute syntax `key=value` is shorthand for a nested object.
> The `=` binds tighter than whitespace — no spaces around it.
> 
> ```compare
> /// styx
> // Shorthand
> server host=localhost port=8080
> /// styx
> // Canonical
> server {
>   host localhost
>   port 8080
> }
> ```

> r[shorthand.attr.value]
> Attribute values may be scalars, sequences, or block objects.
> 
> ```styx
> config name=app tags=(web prod) opts={ verbose true }
> ```

> r[shorthand.attr.termination]
> Attributes continue until a non-`key=...` token. Newlines end the attribute sequence.

### Implicit root

> r[shorthand.root]
> Top-level entries don't require braces — the document is an implicit object.
> 
> ```compare
> /// styx
> // Shorthand
> name "my-app"
> version 1.0
> /// styx
> // Canonical
> {
>   name "my-app"
>   version 1.0
> }
> ```
