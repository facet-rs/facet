---
weight = 5
slug = "diagnostics-spec"
---

# Diagnostics

This section specifies the format and content of error messages. Clear, actionable
diagnostics are essential for a human-authored format.

## Diagnostic format

STYX implementations SHOULD emit diagnostics in the following format:

```
level: message
  --> file:line:column
   |
NN | source line
   | ^^^ annotation
   |
   = note: additional context
   = help: suggested fix
```

> r[diagnostic.format]
> A diagnostic SHOULD include:
> 
> - **Level**: `error`, `warning`, or `note`
> - **Message**: A concise description of the problem
> - **Location**: File path, line number, and column
> - **Source context**: The relevant source line(s) with underline annotations
> - **Help**: When applicable, a concrete suggestion for fixing the problem
> 
> Secondary locations (e.g., "first defined here") use `------` underlines.
> Primary locations (the actual error site) use `^^^^^` underlines.

> r[diagnostic.actionable]
> Error messages SHOULD be actionable. When a fix is known, the diagnostic
> SHOULD show the corrected code, not just describe the problem.
> 
> **Note (non-normative)**: For schema validation errors, diagnostics can be improved
> by including a note pointing to the schema declaration that applied the schema.
> 
> ```
> error: missing required field 'host'
>   --> config.styx:1:1
>   |
> 1 | server { ... }
>   | ^^^^^^ missing 'host'
>   |
>   = note: schema validation failed
>   --> config.styx:1:1
>   |
> 1 | @ "./server.schema.styx"
>   | ^^^^^^^^^^^^^^^^^^^^^^^^ schema applied here
> ```

## Parser errors

### Unexpected token

> r[diagnostic.parser.unexpected]
> When the parser encounters an unexpected token, the message SHOULD identify
> what was found and what was expected.
> 
> ```
> error: unexpected token
>   --> config.styx:3:5
>   |
> 3 |     = value
>   |     ^ expected key or '}'
> ```

### Unclosed delimiter

> r[diagnostic.parser.unclosed]
> When a delimiter is not closed, the message SHOULD show where the opening
> delimiter was and where the parser expected the closing delimiter.
> 
> ```
> error: unclosed '{'
>   --> config.styx:1:8
>   |
> 1 | server {
>   |        ^ unclosed delimiter
>   |
> ...
>   |
> 5 | database {
>   | -------- this '{' might be the problem (missing '}' before it?)
> ```

### Invalid escape sequence

> r[diagnostic.parser.escape]
> When a quoted scalar contains an invalid escape sequence, the message SHOULD
> identify the specific invalid escape.
> 
> ```
> error: invalid escape sequence '\q'
>   --> config.styx:2:12
>   |
> 2 |   name "foo\qbar"
>   |            ^^ invalid escape
>   |
>   = help: valid escapes are: \\, \", \n, \r, \t, \0, \uXXXX, \u{X...}
> ```

### Unterminated string

> r[diagnostic.parser.unterminated-string]
> When a quoted scalar is not terminated, the message SHOULD show where the
> string started.
> 
> ```
> error: unterminated string
>   --> config.styx:2:8
>   |
> 2 |   name "hello
>   |        ^ string starts here
> 3 |   port 8080
>   |
>   = help: add closing '"' or use a heredoc for multiline strings
> ```

### Unterminated heredoc

> r[diagnostic.parser.unterminated-heredoc]
> When a heredoc is not terminated, the message SHOULD show the expected
> delimiter and where the heredoc started.
> 
> ```
> error: unterminated heredoc, expected 'EOF'
>   --> config.styx:2:10
>   |
> 2 |   script <<EOF
>   |          ^^^^^ heredoc starts here
>   |
>   = note: reached end of file while looking for 'EOF'
>   = help: the closing delimiter must appear on its own line
> ```

### Heredoc delimiter too long

> r[diagnostic.parser.heredoc-delimiter-length]
> When a heredoc delimiter exceeds 16 characters, the message SHOULD state
> the limit.
> 
> ```
> error: heredoc delimiter too long
>   --> config.styx:2:10
>   |
> 2 |   script <<THIS_DELIMITER_IS_WAY_TOO_LONG
>   |          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ 35 characters
>   |
>   = help: delimiter must be at most 16 characters
> ```

### Heredoc indentation error

> r[diagnostic.parser.heredoc-indent]
> When a heredoc content line is less indented than the closing delimiter,
> the message SHOULD show both locations.
> 
> ```
> error: heredoc line less indented than closing delimiter
>   --> config.styx:4:1
>   |
> 3 |     script <<BASH
> 4 | echo "hello"
>   | ^^^^ this line has no indentation
> 5 |     BASH
>   |     ---- closing delimiter is indented 4 spaces
>   |
>   = help: indent content to at least column 5, or dedent the closing delimiter
> ```

### Comment without preceding whitespace

> r[diagnostic.parser.comment-whitespace]
> When `//` appears without preceding whitespace (making it part of a scalar),
> the parser cannot distinguish user intent. If subsequent parsing fails, the
> message SHOULD note the potential comment issue.
> 
> ```
> error: unexpected token 'comment'
>   --> config.styx:2:13
>   |
> 2 |   url foo// comment
>   |             ^^^^^^^ unexpected token
>   |
>   = note: '//' without preceding space is part of the scalar 'foo//'
>   = help: add a space before '//' to start a comment
> ```

### Duplicate key

> r[diagnostic.parser.duplicate-key]
> When a key appears twice in the same object, the message SHOULD show both
> locations.
> 
> ```
> error: duplicate key 'port'
>   --> config.styx:4:3
>   |
> 2 |   port 8080
>   |   ---- first defined here
>   |
> 4 |   port 9090
>   |   ^^^^ duplicate key
> ```

### Mixed separators

> r[diagnostic.parser.mixed-separators]
> When an object mixes comma and newline separators, the message SHOULD
> identify both styles and suggest picking one.
> 
> ```
> error: mixed separators in object
>   --> config.styx:2:7
>   |
> 1 | {
> 2 |   a 1,
>   |      ^ comma here
> 3 |   b 2
>   |
>   = help: use either commas or newlines, not both:
>   |
>   | { a 1, b 2 }        // comma-separated
>   |
>   | {                   // newline-separated
>   |   a 1
>   |   b 2
>   | }
> ```

### Comma in sequence

> r[diagnostic.parser.sequence-comma]
> When a comma appears in a sequence, the message SHOULD explain that
> sequences use whitespace separation.
> 
> ```
> error: unexpected ',' in sequence
>   --> config.styx:1:3
>   |
> 1 | (a, b, c)
>   |   ^ commas not allowed in sequences
>   |
>   = help: use whitespace to separate elements: (a b c)
> ```

### Attribute object in sequence

> r[diagnostic.parser.attr-in-sequence]
> When an attribute object appears as a direct sequence element, the message
> SHOULD explain the ambiguity and suggest block form.
> 
> ```
> error: attribute object not allowed as sequence element
>   --> config.styx:2:3
>   |
> 2 |   a=1 b=2
>   |   ^^^^^^^ attribute object
>   |
>   = note: ambiguous whether this is one object {a:1, b:2} or two {a:1} {b:2}
>   = help: use block form: { a 1, b 2 }
> ```

### Trailing content after root

> r[diagnostic.parser.trailing-content]
> When content appears after a closed root object, the message SHOULD note
> that explicit root objects cannot have siblings.
> 
> ```
> error: unexpected token after root object
>   --> config.styx:4:1
>   |
> 1 | {
>   | - root object starts here
> 2 |   key value
> 3 | }
>   | - root object ends here
> 4 | extra
>   | ^^^^^ unexpected token
>   |
>   = help: remove the '{ }' to allow multiple top-level entries
> ```

## Deserializer errors

### Type mismatch

> r[diagnostic.deser.type-mismatch]
> When a scalar cannot be interpreted as the target type, the message SHOULD
> identify the expected type and the actual value.
> 
> ```
> error: type mismatch
>   --> config.styx:2:8
>   |
> 2 |   port "eight thousand"
>   |        ^^^^^^^^^^^^^^^^ expected integer, found string
>   |
>   = help: use a numeric value: port 8080
> ```

### Invalid integer

> r[diagnostic.deser.invalid-integer]
> When a scalar looks like an integer but is invalid (overflow, invalid chars),
> the message SHOULD be specific.
> 
> ```
> error: integer out of range
>   --> config.styx:2:8
>   |
> 2 |   port 99999999999999999999
>   |        ^^^^^^^^^^^^^^^^^^^^ value exceeds u16 maximum (65535)
> ```

### Invalid duration

> r[diagnostic.deser.invalid-duration]
> When a scalar cannot be parsed as a duration, the message SHOULD show
> valid duration formats.
> 
> ```
> error: invalid duration
>   --> config.styx:2:11
>   |
> 2 |   timeout 30 seconds
>   |           ^^ expected duration with unit
>   |
>   = help: valid formats: 30s, 10ms, 2h, 500us
>   = help: valid units: ns, us, µs, ms, s, m, h, d
> ```

### Invalid timestamp

> r[diagnostic.deser.invalid-timestamp]
> When a scalar cannot be parsed as an RFC 3339 timestamp, the message SHOULD
> identify the problem.
> 
> ```
> error: invalid timestamp
>   --> config.styx:2:12
>   |
> 2 |   created 2026-13-01T00:00:00Z
>   |                ^^ month must be 01-12
>   |
>   = help: expected RFC 3339 format: YYYY-MM-DDTHH:MM:SSZ
> ```

### Invalid boolean

> r[diagnostic.deser.invalid-boolean]
> When a value is expected to be boolean but isn't `true` or `false`, the
> message SHOULD list the valid values.
> 
> ```
> error: invalid boolean
>   --> config.styx:2:11
>   |
> 2 |   enabled yes
>   |           ^^^ expected 'true' or 'false'
> ```

### Enum not a single-key object

> r[diagnostic.deser.enum-invalid]
> When deserializing an enum and the value is not a valid tagged value, the
> message SHOULD explain enum representation.
> 
> ```
> error: expected enum variant (tagged value)
>   --> config.styx:2:10
>   |
> 2 |   status "ok"
>   |          ^^^^ expected tag like @ok, found scalar
>   |
>   = help: enum values use tag syntax:
>   |
>   | status @ok              // unit variant
>   | status @err{msg "x"}    // variant with payload
> ```

### Unknown enum variant

> r[diagnostic.deser.unknown-variant]
> When an enum variant name doesn't match any defined variant, the message
> SHOULD list the valid variants.
>
> ```
> error: unknown variant 'unknown'
>   --> config.styx:2:10
>   |
> 2 |   status @unknown
>   |          ^^^^^^^^ not a valid variant
>   |
>   = help: valid variants are: ok, pending, err
> ```

### Missing required field

> r[diagnostic.deser.missing-field]
> When a required field is missing during deserialization, the message SHOULD
> identify the field and the containing object.
> 
> ```
> error: missing required field 'port'
>   --> config.styx:1:1
>   |
> 1 | server {
>   | ^^^^^^ in this object
> 2 |   host localhost
> 3 | }
>   |
>   = help: add the required field: port 8080
> ```

### Unknown field

> r[diagnostic.deser.unknown-field]
> When a field is present but not expected by the target type, the message
> SHOULD suggest similar field names if available.
> 
> ```
> error: unknown field 'prot'
>   --> config.styx:3:3
>   |
> 3 |   prot 8080
>   |   ^^^^ unknown field
>   |
>   = help: did you mean 'port'?
>   = note: expected fields: host, port, timeout
> ```

### Expected object, found scalar

> r[diagnostic.deser.expected-object]
> When an object is expected but a scalar is found.
> 
> ```
> error: expected object, found scalar
>   --> config.styx:2:10
>   |
> 2 |   server localhost
>   |          ^^^^^^^^^ expected object
>   |
>   = help: use braces for object: server { host localhost }
> ```

### Expected sequence, found scalar

> r[diagnostic.deser.expected-sequence]
> When a sequence is expected but a scalar is found.
> 
> ```
> error: expected sequence, found scalar
>   --> config.styx:2:9
>   |
> 2 |   hosts localhost
>   |         ^^^^^^^^^ expected sequence
>   |
>   = help: use parentheses for sequence: hosts (localhost)
> ```

## Schema validation errors

### Type constraint violation

> r[diagnostic.schema.type-violation]
> When a value doesn't match the schema's type constraint.
>
> ```
> error: schema violation: expected @u16
>   --> config.styx:2:8
>   |
> 2 |   port "eighty"
>   |        ^^^^^^^^ cannot parse as u16
>   |
>   --> schema.styx:3:8
>   |
> 3 |   port @u16
>   |        ---- required by schema
> ```

### Literal mismatch

> r[diagnostic.schema.literal-mismatch]
> When a value doesn't match a literal constraint in the schema.
> 
> ```
> error: schema violation: expected literal 'v1', found 'v2'
>   --> config.styx:1:9
>   |
> 1 | version v2
>   |         ^^ expected 'v1'
>   |
>   --> schema.styx:1:9
>   |
> 1 | version v1
>   |         -- literal value required by schema
> ```

### Missing required field (schema)

> r[diagnostic.schema.missing-required]
> When a required field per the schema is missing.
> 
> ```
> error: missing required field 'host'
>   --> config.styx:1:1
>   |
> 1 | server {
>   | ^^^^^^ missing 'host'
> 2 |   port 8080
> 3 | }
>   |
>   --> schema.styx:2:3
>   |
> 2 |   host @string
>   |   ---- required field defined here (no '?' suffix)
> ```

### Unexpected field (schema)

> r[diagnostic.schema.unexpected-field]
> When a field is present but not defined in the schema.
> 
> ```
> error: unexpected field 'debug'
>   --> config.styx:4:3
>   |
> 4 |   debug true
>   |   ^^^^^ not defined in schema
>   |
>   --> schema.styx:1:1
>   |
> 1 | server {
>   | ------ schema for 'server' defined here
>   |
>   = note: schema defines: host, port, timeout
> ```

### Union type mismatch

> r[diagnostic.schema.union-mismatch]
> When a value doesn't match any type in a union.
> 
> ```
> error: value matches no type in union
>   --> config.styx:2:10
>   |
> 2 |   timeout (30 seconds)
>   |           ^^^^^^^^^^^^^ none of the union types match
>   |
>   --> schema.styx:3:11
>   |
> 3 |   timeout? @union(@duration @u64)
>   |            ---------------------- expected one of these types
>   |
>   = note: tried @duration: invalid duration format
>   = note: tried @u64: expected scalar, found sequence
> ```

### Flatten collision

> r[diagnostic.schema.flatten-collision]
> When flattened fields collide with the containing object's fields.
> 
> ```
> error: field collision in @flatten
>   --> schema.styx:8:3
>   |
> 3 |   name @string
>   |   ---- 'name' defined in Base
>   |
> ...
>   |
> 8 |   name @string
>   |   ^^^^ 'name' also defined in Derived
>   |
>   --> schema.styx:7:3
>   |
> 7 |   base @flatten(@Base)
>   |   ---- Base is flattened here
> ```

### Unknown type reference

> r[diagnostic.schema.unknown-type]
> When a type reference cannot be resolved, the message SHOULD be a warning,
> since the type may come from an external source. The unknown type reference
> MUST be treated as `@any`.
> 
> ```
> warning: unknown type '@ExternalConfig'
>   --> schema.styx:5:10
>   |
> 5 |   config @ExternalConfig
>   |          ^^^^^^^^^^^^^^^ type not defined in this schema
>   |
>   = note: treating as @any; validation will be skipped for this field
> ```

> r[diagnostic.schema.unknown-type.strict]
> Implementations MAY provide a **strict mode** for schema validation. In strict
> mode, an unknown type reference MUST be an error, not a warning. This is
> recommended for continuous integration environments to catch typos or missing
> schema definitions.

### Invalid type name

> r[diagnostic.schema.invalid-type-name]
> When a type reference doesn't match the type name grammar.
> 
> ```
> error: invalid type name
>   --> schema.styx:2:8
>   |
> 2 |   port @123-invalid
>   |        ^^^^^^^^^^^^ type names must start with a letter or underscore
>   |
>   = help: valid examples: @string, @MyType, @my_type, @my-type
> ```

### Doc comment without attachment

> r[diagnostic.schema.unattached-doc]
> When a doc comment is not followed by a definition.
> 
> ```
> error: doc comment has no attachment
>   --> schema.styx:3:1
>   |
> 2 | /// This comment is orphaned
> 3 | 
>   | ^ blank line breaks doc comment attachment
> 4 | /// This attaches to 'bar'
> 5 | bar @string
>   |
>   = help: remove the blank line, or delete the orphaned comment
> ```
