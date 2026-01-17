# Styx for Helix

Styx language support for [Helix](https://helix-editor.com/).

## Installation

### 1. Configure the language

Add the contents of `languages.toml` to your Helix config:

```bash
cat /path/to/styx/editors/helix-styx/languages.toml >> ~/.config/helix/languages.toml
```

Or merge manually if you have existing language configurations.

### 2. Install the tree-sitter grammar

```bash
hx --grammar fetch
hx --grammar build
```

### 3. Add queries

Copy or symlink the tree-sitter queries:

```bash
mkdir -p ~/.config/helix/runtime/queries/styx
ln -s /path/to/styx/crates/tree-sitter-styx/queries/* ~/.config/helix/runtime/queries/styx/
```

## Requirements

The Styx CLI must be installed:

```bash
cargo install styx-cli
```

## Features

- Syntax highlighting via tree-sitter
- LSP integration (diagnostics, hover, completions)
- Auto-pairs for brackets and quotes
- Comment toggling with `Ctrl-c`
