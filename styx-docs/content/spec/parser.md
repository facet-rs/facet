+++
title = "Parser"
weight = 2
slug = "parser"
insert_anchor_links = "heading"
+++

The parser converts STYX source text into a document tree.

## Document structure

A STYX document is an object. Top-level entries do not require braces.

> r[document.root]
> The parser MUST interpret top-level key-value pairs as entries of an implicit root object.
> Root entries follow the same separator rules as block objects: newlines or commas (see `r[object.separators]`).
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
> Comments MUST either start at the beginning of the file or be preceded by whitespace.
>
> ```styx
> // comment at start-of-file
> host localhost  // comment
> url https://example.com  // the :// is not a comment
> ```

> r[comment.doc]
> Doc comments start with `///` and attach to the following entry.
> Consecutive doc comment lines are concatenated.
> A doc comment not followed by an entry (blank line or EOF) is an error.
>
> ```styx
> /// The server configuration.
> /// Supports TLS and HTTP/2.
> server {
>   /// Hostname to bind to.
>   host @string
> }
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
> A tag MUST match the pattern `@[A-Za-z_][A-Za-z0-9_.-]*`.
> The tag identifier is terminated by any character not in `[A-Za-z0-9_.-]`, or end of input.
> After termination, the parser checks for an immediate payload (see `r[tag.payload]`).

> r[tag.payload]
> A tag MAY be immediately followed (no whitespace) by an explicit payload:
>
> | Follows `@tag` | Result |
> |-----------------------|--------|
> | `{...}` | tagged object |
> | `(...)` | tagged sequence |
> | `"..."`, `r#"..."#`, or `<<HEREDOC` | tagged scalar |
> | `@` | tagged unit (explicit) |
> | *(nothing)* | tagged unit (implicit) |
>
> ```styx
> result @err{message "x"}   // tagged object
> color @rgb(255 128 0)      // tagged sequence
> name @nickname"Bob"        // tagged scalar
> status @ok@                // tagged unit (explicit)
> status @ok                 // tagged unit (implicit)
> ```
>
> Note: bare scalars cannot be tagged — there's no delimiter to separate tag from value.

## Scalars

Scalars are opaque text atoms. The parser assigns no meaning to them.

### Bare scalars

> r[scalar.bare.chars]
> A bare scalar consists of one or more characters that are NOT:
> whitespace, `{`, `}`, `(`, `)`, `,`, `"`, `=`, or `@`.
>
> r[scalar.bare.termination]
> A bare scalar is terminated by any character not allowed in `r[scalar.bare.chars]`, or end of input.
>
> ```styx
> url https://example.com/path?query=1
> ```

### Quoted scalars

> r[scalar.quoted.escapes]
> Quoted scalars use `"..."` and support escape sequences:
> `\\`, `\"`, `\n`, `\r`, `\t`, `\0`, `\uXXXX`, `\u{X...}`.
> Quoting does not imply string type — the deserializer interprets based on target type.
>
> ```styx
> greeting "hello\nworld"
> port "8080"  // can deserialize as integer
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
> The closing delimiter line MAY be indented; that indentation controls optional indentation stripping.
> Let `indent` be the exact leading whitespace (spaces and/or tabs) immediately before the closing delimiter.
> For each content line, if the line begins with `indent`, that prefix is removed; otherwise the line is left unchanged.
> Content is literal — escape sequences are not processed.
>
> ```styx
> script <<BASH
>   echo "hello"
>   BASH
> ```

## Sequences

> r[sequence.syntax]
> Sequences use `(` `)` delimiters. Empty sequences `()` are valid.
> Elements are separated by one or more whitespace characters (spaces, tabs, or newlines).
> Commas are NOT allowed.
>
> ```styx
> numbers (1 2 3)
> nested ((a b) (c d))
> matrix (
>   (1 2 3)
>   (4 5 6)
> )
> ```

> r[sequence.elements]
> Elements may be scalars, objects, sequences, unit, or tagged values.

## Objects

> r[object.syntax]
> Objects use `{` `}` delimiters. Empty objects `{}` are valid.
> Entries are `key value` pairs separated by newlines or commas (not both).
> Duplicate keys are forbidden (see `r[key.equality]` for key comparison rules).
>
> r[object.separators]
> An object (and the implicit document root) MUST use exactly one top-level entry separator mode:
>
> - **newline-separated**: entries are separated by one or more newlines; commas are forbidden.
> - **comma-separated**: entries are separated by commas; newlines are forbidden (outside heredoc scalar content).
>
> This makes comma-separated objects a single-line representation (except for heredoc content).
>
> ```styx
> server {
>   host localhost
>   port 8080
> }
> { a 1, b 2, c 3 }         // comma-separated
> { "key with spaces" 42 }  // quoted key
> ```

> r[object.keys]
> Keys MUST be either scalars or unit, optionally tagged.
> Heredoc scalars are not allowed as keys.
>
> ```styx
> host localhost            // scalar key
> "key with spaces" 42      // quoted scalar key
> @ mapped                  // unit key
> @root schema              // tagged unit key (implicit unit payload)
> @env"PATH" "/usr/bin"     // tagged scalar key (requires quoted/raw scalar payload)
> ```

> r[key.equality]
> To detect duplicate keys, the parser MUST compare keys by their parsed key value:
>
> - **Untagged scalar keys** compare equal if their scalar contents are exactly equal after parsing
>   (i.e. quoted scalars are compared after escape processing; raw scalars are compared literally).
> - **Untagged unit keys** compare equal to other untagged unit keys.
> - **Tagged keys** compare equal if both the tag name and the parsed payload (scalar or unit) are equal.
>   Implicit unit payloads (e.g. `@ok`) and explicit unit payloads (e.g. `@ok@`) are equivalent.

> r[object.implicit-unit]
> A key without a value has implicit unit value.
>
> ```compare
> /// styx
> // Shorthand
> enabled
> /// styx
> // Canonical
> enabled @
> ```

## Unit

> r[value.unit]
> The token `@` not followed by an identifier is the unit value.
>
> ```styx
> enabled @
> ```

## Shorthand syntax

### Attribute objects

> r[shorthand.attr]
> Attribute syntax `key=value` is shorthand for a nested object.
> The `=` binds tighter than whitespace — no spaces around it.
> The `=` token MUST only appear as part of attribute syntax; a standalone `=` is an error.
> Attribute keys MUST be bare scalars.
> If an entry key is followed by one or more attributes, the entry's value is the corresponding object.
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
> config name=app tags=(web prod) opts={verbose true}
> ```

> r[shorthand.attr.termination]
> Attributes continue until the end of the current entry (newline or comma) or until a non-`key=...` token.

## Appendix: Minified STYX

STYX does not strictly require newlines. A document can be written on a single line using commas and explicit braces:

```styx
{server{host localhost,port 8080,tags(web prod)},database{url "postgres://..."}}
```

This is equivalent to:

```styx
server {
  host localhost
  port 8080
  tags (web prod)
}

database {
  url "postgres://..."
}
```

This enables NDSTYX (newline-delimited STYX), analogous to NDJSON — one document per line for streaming or log-style data:

```
{event login,user alice,time 2026-01-12T10:00:00Z}
{event logout,user alice,time 2026-01-12T10:30:00Z}
{event login,user bob,time 2026-01-12T10:45:00Z}
```
