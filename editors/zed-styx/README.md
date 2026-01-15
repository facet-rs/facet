# Styx Extension for Zed

Syntax highlighting and language support for the [Styx](https://github.com/bearcove/styx) configuration language in [Zed](https://zed.dev).

## Features

- Syntax highlighting via tree-sitter
- Bracket matching and auto-closing
- Automatic indentation
- Language injection in heredocs (e.g., SQL, JSON, HTML)

## Installation

### From Zed Extensions (Recommended)

1. Open Zed
2. Open the Extensions panel (`cmd+shift+x` or `View > Extensions`)
3. Search for "Styx"
4. Click Install

### Manual Installation

```bash
# Clone the repository
git clone https://github.com/bearcove/styx
cd styx/editors/zed-styx

# Copy to Zed extensions directory
mkdir -p ~/.config/zed/extensions/installed/styx
cp -r . ~/.config/zed/extensions/installed/styx/
```

## Heredoc Language Injection

Styx supports heredocs with language hints for syntax highlighting:

```styx
query <<SQL,sql
SELECT * FROM users
WHERE active = true
SQL

template <<HTML,html
<div class="container">
  <h1>Hello, World!</h1>
</div>
HTML
```

The language after the comma (e.g., `sql`, `html`) triggers syntax highlighting for that language within the heredoc content.

## Development

To work on the extension locally:

```bash
# Link the extension for development
ln -s $(pwd)/editors/zed-styx ~/.config/zed/extensions/installed/styx

# After making changes to the tree-sitter grammar:
cd crates/tree-sitter-styx
npx tree-sitter generate

# Restart Zed to pick up changes
```

## Related

- [Styx Language](https://github.com/bearcove/styx) - The Styx configuration language
- [tree-sitter-styx](../../crates/tree-sitter-styx) - Tree-sitter grammar for Styx
