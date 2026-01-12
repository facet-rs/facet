# Phase 001: tree-sitter-styx

Tree-sitter grammar for Styx, enabling syntax highlighting and structural queries in editors.

## Deliverables

- `crates/tree-sitter-styx/grammar.js` - Tree-sitter grammar definition
- `crates/tree-sitter-styx/src/` - Generated parser (C code)
- `crates/tree-sitter-styx/queries/highlights.scm` - Syntax highlighting queries
- `crates/tree-sitter-styx/corpus/` - Test cases

## Grammar Mapping

### Node Types

| Styx Concept | Tree-sitter Node |
|--------------|------------------|
| Document | `document` (root) |
| Entry | `entry` |
| Object | `object` |
| Sequence | `sequence` |
| Bare scalar | `bare_scalar` |
| Quoted scalar | `quoted_scalar` |
| Raw scalar | `raw_scalar` |
| Heredoc | `heredoc` |
| Unit | `unit` |
| Tag | `tag` |
| Tag name | `tag_name` |
| Attributes | `attributes` |
| Attribute | `attribute` |
| Key (in entry) | `key` (field name) |
| Value (in entry) | `value` (field name) |
| Doc comment | `doc_comment` |
| Line comment | `comment` |

### Challenges

1. **Heredoc delimiters** - Tree-sitter has `externals` for context-sensitive parsing. Use external scanner for heredoc matching.

2. **Raw string `#` matching** - Same approach: external scanner to count `#` on open and match on close.

3. **Entry key-path flattening** - Tree-sitter doesn't need to flatten; it can represent `a b c` as three atoms in an entry. The semantic interpretation (nested keys) is for higher layers.

4. **Separator mode detection** - Not needed for tree-sitter; it just parses structure. Semantic validation is separate.

## Implementation Steps

### Step 1: Basic grammar.js

Define core structure without externals:
- Document, Entry, Object, Sequence
- Bare, Quoted scalars
- Unit, Tags
- Comments

### Step 2: External scanner

Implement `src/scanner.c` for:
- Heredoc open/close with delimiter matching
- Raw string `#` counting

### Step 3: Highlight queries

Create `queries/highlights.scm`:
```scheme
(bare_scalar) @string
(quoted_scalar) @string
(raw_scalar) @string
(heredoc) @string

(tag_name) @type
(unit) @constant.builtin

(comment) @comment
(doc_comment) @comment.documentation

; Keys in entries
(entry key: (_) @property)

; Punctuation
"{" @punctuation.bracket
"}" @punctuation.bracket
"(" @punctuation.bracket
")" @punctuation.bracket
"@" @punctuation.special
"=" @operator
```

### Step 4: Test corpus

Create test files covering:
- Simple scalars
- Nested objects
- Sequences
- Mixed separator modes
- Heredocs with various delimiters
- Raw strings with varying `#` counts
- Tags with all payload types
- Attributes
- Comments and doc comments
- Edge cases from spec

## Validation

- `tree-sitter generate` succeeds
- `tree-sitter test` passes all corpus tests
- `tree-sitter highlight` produces sensible output on example files
- Integration with arborium works

## Notes

- Tree-sitter grammars are JavaScript, but generate C parsers
- The Rust bindings are auto-generated
- Arborium will use the generated parser for highlighting
