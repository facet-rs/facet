# Styx Compliance Suite - S-Expression Format

Version: `2026-01-16`

## Overview

The compliance suite uses S-expressions to represent parse trees. This format is:
- Easy to emit from any language (just string concatenation)
- Human-readable
- Diff-friendly
- Battle-tested (used by tree-sitter)

## Grammar

```
tree     = "(" "document" span value* ")"
value    = unit | scalar | tag | sequence | object
unit     = "(" "unit" span ")"
scalar   = "(" "scalar" span kind string ")"
tag      = "(" "tag" span string payload? ")"
sequence = "(" "sequence" span value* ")"
object   = "(" "object" span sep entry* ")"
entry    = "(" "entry" key value ")"
key      = value
payload  = value

span     = "[" number "," number "]"
sep      = "newline" | "comma"
kind     = "bare" | "quoted" | "raw" | "heredoc"
string   = <JSON-escaped string in double quotes>
number   = <decimal integer>
```

## Node Types

### document

The root node. Contains zero or more values (entries at document level).

```scheme
(document [0, 42]
  (entry ...)
  (entry ...))
```

### unit

The unit value `@` — no tag, no payload.

```scheme
(unit [5, 6])
```

### scalar

A scalar value with its kind and text content.

```scheme
(scalar [0, 5] bare "hello")
(scalar [0, 7] quoted "hello")
(scalar [0, 12] raw "C:\\path")
(scalar [0, 25] heredoc "line1\nline2")
```

The text is always the **interpreted** value (escapes processed), JSON-escaped for output.

### tag

A tagged value. The tag name is a string. Payload is optional.

```scheme
; @ok (no payload)
(tag [0, 3] "ok")

; @error{code 500}
(tag [0, 16] "error"
  (object [6, 16] newline
    (entry
      (scalar [7, 11] bare "code")
      (scalar [12, 15] bare "500"))))

; @rgb(255 128 0)
(tag [0, 15] "rgb"
  (sequence [4, 15]
    (scalar [5, 8] bare "255")
    (scalar [9, 12] bare "128")
    (scalar [13, 14] bare "0")))
```

### sequence

A sequence of values.

```scheme
(sequence [0, 9]
  (scalar [1, 2] bare "a")
  (scalar [3, 4] bare "b")
  (scalar [5, 6] bare "c"))
```

### object

An object with entries. Includes separator style (`newline` or `comma`).

```scheme
(object [0, 24] newline
  (entry
    (scalar [1, 5] bare "name")
    (scalar [6, 11] bare "hello"))
  (entry
    (scalar [12, 16] bare "port")
    (scalar [17, 21] bare "8080")))
```

### entry

A key-value pair in an object. Keys can be any value (usually scalar, sometimes unit `@`).

```scheme
(entry
  (scalar [0, 4] bare "name")
  (scalar [5, 10] bare "value"))

; Unit key (schema declaration)
(entry
  (unit [0, 1])
  (scalar [2, 20] bare "path/to/schema.styx"))
```

## Spans

Spans are byte offsets `[start, end)` (start inclusive, end exclusive).

For values without source position (programmatically constructed), use `[-1, -1]`.

## String Escaping

All strings in the output use JSON escaping:
- `"` → `\"`
- `\` → `\\`
- newline → `\n`
- tab → `\t`
- carriage return → `\r`
- other control characters → `\uXXXX`

## Whitespace

Implementations SHOULD format output with:
- Newlines after each top-level node
- 2-space indentation for nesting
- Single space between tokens on same line

However, for comparison purposes, implementations MAY normalize whitespace before diffing.

## Multi-File Output

When processing multiple files, output trees consecutively with file markers:

```scheme
; file: corpus/00-basic/empty.styx
(document [0, 0])

; file: corpus/00-basic/single.styx  
(document [0, 12]
  (entry
    (scalar [0, 4] bare "name")
    (scalar [5, 10] bare "hello")))
```

## Error Cases

For files that fail to parse, output an error node:

```scheme
; file: corpus/08-invalid/unclosed.styx
(error [0, 10] "expected closing brace")
```

The span indicates where the error was detected. The message is implementation-defined but should be present.

## Example

Input (`example.styx`):
```styx
@ schema.styx

name "My Config"
enabled true
server {host localhost, port 8080}
```

Output:
```scheme
(document [0, 79]
  (entry
    (unit [0, 1])
    (scalar [2, 13] bare "schema.styx"))
  (entry
    (scalar [15, 19] bare "name")
    (scalar [20, 31] quoted "My Config"))
  (entry
    (scalar [32, 39] bare "enabled")
    (tag [40, 45] "true"))
  (entry
    (scalar [46, 52] bare "server")
    (object [53, 79] comma
      (entry
        (scalar [54, 58] bare "host")
        (scalar [59, 68] bare "localhost"))
      (entry
        (scalar [70, 74] bare "port")
        (scalar [75, 79] bare "8080")))))
```
