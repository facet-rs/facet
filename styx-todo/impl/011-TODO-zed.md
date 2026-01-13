# Phase 011: zed-styx (Zed Extension)

Zed extension for Styx, providing syntax highlighting via tree-sitter and LSP integration.

## Deliverables

- `editors/zed-styx/` - Extension package
- `editors/zed-styx/extension.toml` - Extension manifest
- `editors/zed-styx/languages/styx/` - Language definition
- `editors/zed-styx/languages/styx/config.toml` - Language config
- `editors/zed-styx/languages/styx/highlights.scm` - Tree-sitter highlights

## Features

### Tree-sitter Integration

Uses `tree-sitter-styx` for:
- Syntax highlighting
- Code folding
- Bracket matching
- Outline/symbols

### LSP Integration

When styx-ls is available:
- Diagnostics
- Completions
- Hover
- Formatting
- Go to definition

## Package Structure

```
editors/zed-styx/
├── extension.toml
├── languages/
│   └── styx/
│       ├── config.toml
│       ├── highlights.scm
│       ├── injections.scm
│       ├── brackets.scm
│       ├── indents.scm
│       └── outline.scm
└── README.md
```

## extension.toml

```toml
id = "styx"
name = "Styx"
description = "Styx configuration language support"
version = "0.1.0"
schema_version = 1
authors = ["bearcove <hello@bearcove.net>"]
repository = "https://github.com/bearcove/styx"

[language_servers.styx-ls]
name = "Styx Language Server"
language = "Styx"
command = "styx-ls"

[grammars.styx]
repository = "https://github.com/bearcove/styx"
path = "crates/tree-sitter-styx"
commit = "main"
```

## languages/styx/config.toml

```toml
name = "Styx"
grammar = "styx"
path_suffixes = ["styx"]
line_comments = ["//"]
block_comments = []
autoclose_before = "]})"
brackets = [
    { start = "{", end = "}", close = true, newline = true },
    { start = "(", end = ")", close = true, newline = false },
    { start = "\"", end = "\"", close = true, newline = false, not_in = ["string"] },
]
word_characters = ["_"]
```

## highlights.scm

Tree-sitter highlight queries:

```scheme
; Comments
(line_comment) @comment
(doc_comment) @comment.documentation

; Strings
(quoted_scalar) @string
(raw_scalar) @string
(heredoc) @string
(heredoc_marker) @punctuation.special

; Tags and units
(tag name: (identifier) @type)
(unit) @constant.builtin

; Keys and values
(entry key: (scalar) @property)

; Punctuation
"{" @punctuation.bracket
"}" @punctuation.bracket
"(" @punctuation.bracket
")" @punctuation.bracket
"," @punctuation.delimiter
"=" @operator
"@" @punctuation.special

; Bare scalars that look like numbers
(bare_scalar) @number
(#match? @number "^-?[0-9]+(\\.[0-9]+)?$")

; Bare scalars that look like booleans
(bare_scalar) @constant.builtin
(#match? @constant.builtin "^(true|false)$")

; Other bare scalars
(bare_scalar) @string
```

## brackets.scm

```scheme
("{" @open "}" @close)
("(" @open ")" @close)
```

## indents.scm

```scheme
(object) @indent
(sequence) @indent

"}" @outdent
")" @outdent
```

## outline.scm

For document outline/symbols:

```scheme
(entry
  key: (scalar) @name) @item
```

## injections.scm

For embedded languages (if we support them in heredocs):

```scheme
; Could support syntax highlighting in heredocs based on marker
; (heredoc
;   marker: (heredoc_marker) @language
;   content: (heredoc_content) @content)
```

## LSP Configuration

Users can configure the language server in Zed settings:

```json
{
  "languages": {
    "Styx": {
      "language_servers": ["styx-ls"]
    }
  },
  "lsp": {
    "styx-ls": {
      "binary": {
        "path": "/usr/local/bin/styx-ls"
      }
    }
  }
}
```

## Building & Installation

### From Zed Extensions

Search for "Styx" in Zed's extension marketplace.

### Manual Installation

```bash
# Clone and build
git clone https://github.com/bearcove/styx
cd styx/editors/zed-styx

# Install to Zed extensions directory
mkdir -p ~/.config/zed/extensions/styx
cp -r . ~/.config/zed/extensions/styx/
```

### Development

```bash
# Link extension for development
ln -s $(pwd)/editors/zed-styx ~/.config/zed/extensions/styx

# Rebuild tree-sitter grammar if needed
cd crates/tree-sitter-styx
tree-sitter generate
```

## Testing

- Test highlights with various Styx documents
- Test LSP features work correctly
- Test indentation and bracket matching
- Test outline view shows correct structure
