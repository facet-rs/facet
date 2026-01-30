+++
title = "Parser"
weight = 2
slug = "parser"
insert_anchor_links = "heading"
+++

The parser converts Styx source text into a document tree.

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
> A bare scalar starts with a character that is NOT:
> whitespace, `{`, `}`, `(`, `)`, `,`, `"`, `=`, `@`, or `>`.
>
> After the first character, `@` and `=` are allowed but `>` is still forbidden.
> This allows URLs with `@` (like `user@host` or `crate:pkg@2`) and query strings with `=`.

> r[scalar.bare.termination]
> A bare scalar is terminated by any forbidden character or end of input.
>
> ```styx
> url https://example.com/path
> ```

### Quoted scalars

> r[scalar.quoted.escapes]
> Quoted scalars use `"..."` and support escape sequences:
> `\\`, `\"`, `\n`, `\r`, `\t`, `\uXXXX`, `\u{X...}`.

> r[scalar.quoted.newline]
> The `\n` escape sequence always produces a single LF character (U+000A), regardless of platform.
> Use `\r\n` explicitly if CRLF is needed.
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

> r[scalar.heredoc.invalid]
> A `<<` sequence that is NOT immediately followed by an uppercase letter is a parse error.
> This includes `<<` followed by lowercase letters, digits, whitespace, or end of input.
>
> ```styx,bad
> value <<eof        // ERROR: delimiter must start with uppercase
> value <<123        // ERROR: delimiter must start with uppercase
> value <<           // ERROR: missing delimiter
> ```
>
> Note: A single `<` not followed by another `<` is valid as part of a bare scalar.

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
> A tag MUST match the pattern `@[A-Za-z_][A-Za-z0-9_-]*`.
> Note: dots are NOT allowed in tag names (they are path separators in keys).

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

An **entry** consists of a key and an optional value.

> r[entry.structure]
> An entry has exactly one key and at most one value:
>
> - **1 atom**: the atom is the key, the value is implicit unit (`@`)
> - **2 atoms**: first is key, second is value
>
> ```styx
> enabled                  // enabled = @
> host localhost           // host = localhost
> type @string             // type = @string
> config @object{}         // config = @object{}
> ```

> r[entry.whitespace]
> A bare scalar key MUST be separated from a following `{` or `(` by whitespace.
> This prevents visual confusion with tag syntax (e.g., `@tag{...}`).
>
> ```styx
> config {}                // valid: whitespace before {
> items (1 2 3)            // valid: whitespace before (
> ```
>
> ```styx,bad
> config{}                 // ERROR: missing whitespace before {
> items(1 2 3)             // ERROR: missing whitespace before (
> ```
>
> Note: Quoted scalars, raw scalars, and tags do not have this restriction
> since they have clear delimiters. `@tag{}` is a tagged object (one atom).

> r[entry.toomany]
> An entry with more than two atoms is a parse error.
>
> ```styx,bad
> key @tag {}              // ERROR: 3 atoms
> a b c                    // ERROR: 3 atoms
> ```
>
> A common mistake is putting whitespace between a tag and its payload.
> The error message SHOULD suggest removing the space:
>
> ```
> key @tag {}
>
> Error: unexpected `{` after value
>
> Hint: did you mean `@tag{}`? Whitespace is not allowed between a tag and its payload.
> ```

> r[entry.keys]
> A key is a dotted path of one or more segments. Each segment may be:
> - A bare key (like bare scalar but `.` terminates it)
> - A quoted scalar
> - Unit (`@`)
> - A tag (`@name` or `@name"payload"`)
>
> Objects, sequences, and heredocs are not valid keys.
>
> ```styx
> // Valid keys:
> host localhost            // bare key
> "key with spaces" 42      // quoted key
> @ mapped                  // unit key
> @root schema              // tagged unit key
> @env"PATH" "/usr/bin"     // tagged scalar key
> ```
>
> ```styx,bad
> // Invalid keys:
> {a 1} value               // object as key
> (a b) value               // sequence as key
> <<EOF                     // heredoc as key
> text
> EOF
> value
> ```

> r[entry.path]
> A dotted key defines a nested path. Each segment separated by `.` becomes
> a key in a nested object chain. The value is placed at the innermost level.
>
> ```compare
> /// styx
> // Dotted path
> selector.matchLabels app>web
> /// styx
> // Canonical
> selector {
>   matchLabels {
>     app web
>   }
> }
> ```
>
> ```styx
> a.b.c value              // a { b { c value } }
> server.host localhost    // server { host localhost }
> profile.release.lto true // profile { release { lto true } }
> ```
>
> Quoted segments do not split on dots:
>
> ```styx
> "a.b".c value            // "a.b" { c value }
> ```

> r[entry.path.sibling]
> Sibling dotted paths (paths sharing a common prefix) are allowed as long as
> they appear contiguously. Moving to a different key at any level closes the
> previous sibling path and all its descendants.
>
> ```styx
> // Valid: sibling paths under common prefix
> foo.bar.x value1
> foo.bar.y value2         // foo.bar still open
> foo.baz value3           // foo still open, foo.bar now closed
> ```

> r[entry.path.reopen]
> Reopening a closed path is an error. A path is closed when a sibling path
> at the same level receives an entry.
>
> ```styx,bad
> foo.bar {}
> foo.baz {}               // closes foo.bar
> foo.bar.x value          // ERROR: foo.bar was closed
> ```
>
> ```styx,bad
> a.b.c {}
> a.b.d {}                 // closes a.b.c
> a.x {}                   // closes a.b
> a.b.e {}                 // ERROR: a.b was closed
> ```
>
> This rule enables streaming deserialization: once a different sibling appears,
> the previous subtree is complete and can be finalized without buffering.

> r[entry.key-equality]
> To detect duplicate keys, the parser MUST compare keys by their parsed value:
>
> - **Scalar keys** compare equal if their contents are exactly equal after parsing
>   (quoted scalars are compared after escape processing).
> - **Unit keys** compare equal to other unit keys.
> - **Tagged keys** compare equal if both tag name and payload are equal.

### Separators

> r[object.separators]
> Entries are separated by newlines, commas, or both. Duplicate keys are forbidden.
>
> ```styx
> server {
>   host localhost
>   port 8080
> }
> {a 1, b 2, c 3}
> {a 1, b 2
>  c 3}              // mixed separators allowed
> ```

### Attribute syntax

Attribute syntax is shorthand for inline object entries.

> r[attr.syntax]
> Attribute syntax `key>value` creates an object entry.
> The `>` has no spaces around it.
> Attribute keys MUST be bare scalars.
>
> ```compare
> /// styx
> // Shorthand
> server host>localhost port>8080
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
> config name>app tags>(web prod) opts>{verbose true}
> ```

> r[attr.atom]
> Multiple attributes combine into a single object atom.
>
> ```compare
> /// styx
> host>localhost port>8080
> /// styx
> {host localhost, port 8080}
> ```

> r[entry.path.attributes]
> Dotted paths compose naturally with attribute syntax.
>
> ```compare
> /// styx
> // Path with attributes as value
> spec.selector.matchLabels app>web tier>frontend
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

A Styx document is an object. Top-level entries do not require braces.

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

## Appendix: Minified Styx

Styx can be written on a single line using commas and explicit braces:

```styx
{server {host localhost,port 8080},database {url "postgres://..."}}
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

This enables NDStyx (newline-delimited Styx) for streaming:

```styx
{event login,user alice,time 2026-01-12T10:00:00Z}
{event logout,user alice,time 2026-01-12T10:30:00Z}
```
