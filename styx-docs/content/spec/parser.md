+++
title = "Parser"
weight = 2
slug = "parser"
insert_anchor_links = "heading"
+++

The parser converts STYX source text into a document tree.

## Comments

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

## Atoms

An **atom** is the fundamental parsing unit:

  * **Bare scalar** — unquoted text: `localhost`, `8080`, `https://example.com`
  * **Quoted scalar** — quoted text with escapes: `"hello\nworld"`
  * **Raw scalar** — literal text: `r#"no escapes"#`
  * **Heredoc scalar** — multi-line literal text: `<<EOF...EOF`
  * **Sequence** — ordered list: `(a b c)`
  * **Object** — ordered map: `{key value}`
  * **Unit** — absence of value: `@`
  * **Tag** — labeled value: `@tag`, `@tag(...)`, `@tag{...}`

## Scalars

Scalars are opaque text. The parser assigns no type information.

### Bare scalars

> r[scalar.bare.chars]
> A bare scalar consists of one or more characters that are NOT:
> whitespace, `{`, `}`, `(`, `)`, `,`, `"`, `=`, or `@`.
>
> r[scalar.bare.termination]
> A bare scalar is terminated by any forbidden character or end of input.
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
> The closing delimiter line MAY be indented; that indentation is stripped from content lines.
>
> ```styx
> script <<BASH
>   echo "hello"
>   BASH
> ```

> r[scalar.heredoc.lang]
> A heredoc MAY include a language hint after the delimiter, separated by a comma.
> The language hint MUST match `[a-z][a-z0-9_.-]*` (lowercase identifiers).
> The language hint is metadata and does not affect the scalar content.
>
> ```styx
> code <<EOF,rust
>   fn main() {
>     println!("Hello");
>   }
>   EOF
>
> query <<SQL,sql
>   SELECT * FROM users
>   SQL
> ```

## Unit

> r[value.unit]
> The token `@` not followed by an identifier is the unit value.
>
> ```styx
> enabled @
> ```

## Tags

A tag labels a value with an identifier.

> r[tag.syntax]
> A tag MUST match the pattern `@[A-Za-z_][A-Za-z0-9_.-]*`.

> r[tag.payload]
> A tag MAY be immediately followed (no whitespace) by a payload:
>
> | Follows `@tag` | Result |
> |----------------|--------|
> | `{...}` | tagged object |
> | `(...)` | tagged sequence |
> | `"..."`, `r#"..."#`, `<<HEREDOC` | tagged scalar |
> | `@` | tagged unit (explicit) |
> | *(nothing)* | tagged unit (implicit) |
>
> ```styx
> result @err{message "x"}   // tagged object
> color @rgb(255 128 0)      // tagged sequence
> name @nickname"Bob"        // tagged scalar
> status @ok                 // tagged unit
> ```
>
> Bare scalars cannot be tagged — there's no delimiter to separate tag from value.

## Sequences

> r[sequence.syntax]
> Sequences use `(` `)` delimiters. Empty sequences `()` are valid.
> Elements are separated by whitespace (spaces, tabs, or newlines).
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
> Elements may be any atom type.

## Objects

Objects are ordered collections of entries.

> r[object.syntax]
> Objects use `{` `}` delimiters. Empty objects `{}` are valid.

### Entries

An **entry** is a sequence of one or more atoms. The parser interprets entries structurally:

> r[entry.structure]
> An entry consists of one or more atoms:
>
> - **1 atom**: the atom is the key, the value is implicit unit (`@`)
> - **2 atoms**: first is the key, second is the value
> - **N atoms** (N > 2): first N-1 atoms form a nested key path, last atom is the value
>
> ```styx
> enabled                  // enabled = @
> host localhost           // host = localhost
> server host localhost    // server {host localhost}
> server host port 8080    // server {host {port 8080}}
> ```

> r[entry.keypath]
> When an entry has more than two atoms, the first N-1 atoms are keys forming a nested object path.
> The final atom is the value at the innermost level.
>
> ```compare
> /// styx
> // Key path
> selector matchLabels app web
> /// styx
> // Canonical
> selector {
>   matchLabels {
>     app web
>   }
> }
> ```

> r[entry.keys]
> A key may be any value, tagged or not, except objects, sequences, and heredocs.
>
> ```styx
> // Valid keys:
> host localhost            // bare scalar key
> "key with spaces" 42      // quoted scalar key
> @ mapped                  // unit key
> @root schema              // tagged unit key
> @env"PATH" "/usr/bin"     // tagged scalar key
> ```
>
> ```styx,bad
> // Invalid keys:
> {a 1} value               // object as key
> (1 2 3) value             // sequence as key
> <<EOF                     // heredoc as key
> text
> EOF
> value
> ```

> r[entry.key-equality]
> To detect duplicate keys, the parser MUST compare keys by their parsed value:
>
> - **Scalar keys** compare equal if their contents are exactly equal after parsing
>   (quoted scalars are compared after escape processing).
> - **Unit keys** compare equal to other unit keys.
> - **Tagged keys** compare equal if both tag name and payload are equal.

### Separators

> r[object.separators]
> Entries are separated by newlines or commas. Duplicate keys are forbidden.
> An object MUST use exactly one separator mode:
>
> - **newline-separated**: entries separated by newlines; commas forbidden
> - **comma-separated**: entries separated by commas; newlines forbidden
>
> Comma-separated objects are single-line (except for heredoc content).
>
> ```styx
> server {
>   host localhost
>   port 8080
> }
> {a 1, b 2, c 3}
> ```

### Attribute syntax

Attribute syntax is shorthand for inline object entries.

> r[attr.syntax]
> Attribute syntax `key=value` creates an object entry.
> The `=` has no spaces around it.
> Attribute keys MUST be bare scalars.
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

> r[attr.values]
> Attribute values may be bare scalars, quoted scalars, sequences, or objects.
>
> ```styx
> config name=app tags=(web prod) opts={verbose true}
> ```

> r[attr.atom]
> Multiple attributes combine into a single object atom.
>
> ```compare
> /// styx
> host=localhost port=8080
> /// styx
> {host localhost, port 8080}
> ```

> r[entry.keypath.attributes]
> Key paths compose naturally with attribute syntax.
>
> ```compare
> /// styx
> // Key path with attributes
> spec selector matchLabels app=web tier=frontend
> /// styx
> // Canonical
> spec {
>   selector {
>     matchLabels {
>       app web
>       tier frontend
>     }
>   }
> }
> ```

## Document structure

A STYX document is an object. Top-level entries do not require braces.

> r[document.root]
> The parser MUST interpret top-level entries as entries of an implicit root object.
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

## Appendix: Minified STYX

STYX can be written on a single line using commas and explicit braces:

```styx
{server{host localhost,port 8080},database{url "postgres://..."}}
```

This is equivalent to:

```styx
server {
  host localhost
  port 8080
}

database {
  url "postgres://..."
}
```

This enables NDSTYX (newline-delimited STYX) for streaming:

```
{event login,user alice,time 2026-01-12T10:00:00Z}
{event logout,user alice,time 2026-01-12T10:30:00Z}
```
