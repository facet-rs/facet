# Compliance Suite

**Status:** Done  
**Priority:** High

## Problem

As Styx grows (language bindings, alternative implementations), we need a way to verify parsers produce identical results.

## Goals

1. **Canonical tree format** — Styx itself (dogfooding), compact, versioned
2. **Test corpus** — Edge cases, tricky syntax, real-world examples
3. **Golden vectors** — Expected parse trees for each corpus file
4. **Test runner** — Python script that parses all files and outputs all trees in one invocation (avoids interpreter warmup)

## Design

### Canonical Output Format

S-expressions, like tree-sitter. Version `2026-01-16`.

Easy to emit from any language. Battle-tested format.

```bash
styx @tree --format sexp file.styx
```

```scheme
(document [0, 24]
  (entry [0, 10]
    (key [0, 4] (scalar "name"))
    (value [5, 10] (scalar "hello")))
  (entry [11, 24]
    (key [11, 14] (scalar "port"))
    (value [15, 19] (scalar "8080"))))
```

Format rules:
- `(node_type [start, end] children...)`
- Scalars: `(scalar "text")` — text is JSON-escaped
- Tags: `(tag "name" payload...)` 
- Sequences: `(sequence items...)`
- Objects: `(object entries...)`
- Unit: `(unit)`

### Test Corpus Structure

```
compliance/
├── README.md
├── format.md                 # S-expression format specification
├── corpus/
│   ├── 00-basic/
│   │   ├── empty.styx
│   │   ├── single-entry.styx
│   │   ├── multiple-entries.styx
│   │   └── ...
│   ├── 01-scalars/
│   │   ├── bare.styx
│   │   ├── quoted.styx
│   │   ├── quoted-escapes.styx
│   │   ├── raw.styx
│   │   ├── raw-hashes.styx
│   │   ├── heredoc.styx
│   │   ├── heredoc-lang-hint.styx
│   │   └── ...
│   ├── 02-objects/
│   │   ├── empty.styx
│   │   ├── newline-sep.styx
│   │   ├── comma-sep.styx
│   │   ├── nested.styx
│   │   └── ...
│   ├── 03-sequences/
│   ├── 04-tags/
│   ├── 05-attributes/
│   ├── 06-comments/
│   ├── 07-edge-cases/
│   │   ├── unicode.styx
│   │   ├── deeply-nested.styx
│   │   ├── large-heredoc.styx
│   │   └── ...
│   └── 08-invalid/
│       ├── mixed-separators.styx
│       ├── unclosed-brace.styx
│       └── ...
├── golden/
│   ├── 00-basic/
│   │   ├── empty.tree.styx
│   │   ├── single-entry.tree.styx
│   │   └── ...
│   └── ...
└── golden.styx               # All expected trees concatenated
```

### Invalid File Testing

For `08-invalid/`, golden files contain expected error info:

```styx
@ compliance/error.schema.styx
version 2026-01-16

valid @false
errors (
    @error{
        code E0001
        message_contains "mixed separators"
        span (10 15)
    }
)
```

### Runner

Each implementation provides its own runner that:
1. Takes a directory of `.styx` files
2. Parses all of them in a single process (avoids startup overhead)
3. Outputs all trees to stdout in canonical format

```bash
# Rust reference implementation
cargo run --bin styx-compliance compliance/corpus/ > output.styx

# Python implementation
python -m styx_py.compliance compliance/corpus/ > output.styx

# Node.js implementation  
node styx-js/compliance.mjs compliance/corpus/ > output.styx
```

Then diff against golden:
```bash
diff -u compliance/golden.styx output.styx
```

## Implementation Plan

1. [x] Define s-expression format spec (`compliance/format.md`)
2. [x] Add `styx @tree --format sexp` flag  
3. [x] Create initial corpus (50 files covering basics)
4. [x] Generate golden files from reference implementation
5. [x] Add to CI (`.github/workflows/ci.yml`)
6. [x] Document how other implementations can run the suite (`compliance/README.md`)
7. [ ] Expand corpus over time (fuzzing finds edge cases → add to corpus)

## Future

- Schema validation compliance testing (separate suite)
