+++
title = "Editor Integration"
weight = 1
+++

# Editor Integration

Styx has first-class editor support through LSP and tree-sitter.

## Zed

The Zed extension is built into the repository:

1. Build the extension: `cd editors/zed-styx && cargo build`
2. Install via Zed's extension browser (coming soon to the extension gallery)

## VS Code

```bash
cd editors/vscode-styx
npm install
npm run compile
```

Then press F5 to launch with the extension, or package it:

```bash
npm run package
code --install-extension styx-0.1.0.vsix
```

### Configuration

- `styx.server.path`: Path to styx binary (default: `"styx"`)
- `styx.trace.server`: LSP trace level (`"off"`, `"messages"`, `"verbose"`)

## Neovim

See `editors/nvim-styx/README.md` for full setup instructions.

### Quick Start

1. Add the tree-sitter parser to nvim-treesitter:

```lua
local parser_config = require("nvim-treesitter.parsers").get_parser_configs()
parser_config.styx = {
  install_info = {
    url = "https://github.com/bearcove/styx",
    files = { "crates/tree-sitter-styx/src/parser.c", "crates/tree-sitter-styx/src/scanner.c" },
    location = "crates/tree-sitter-styx",
  },
  filetype = "styx",
}
```

2. Configure the LSP:

```lua
local lspconfig = require("lspconfig")
local configs = require("lspconfig.configs")

if not configs.styx then
  configs.styx = {
    default_config = {
      cmd = { "styx", "@lsp" },
      filetypes = { "styx" },
      root_dir = lspconfig.util.root_pattern(".git", "*.styx"),
    },
  }
end

lspconfig.styx.setup({})
```

## Sublime Text

Copy `editors/sublime-styx` to your Packages folder and symlink the TextMate grammar:

```bash
cd ~/Library/Application\ Support/Sublime\ Text/Packages
mkdir Styx && cd Styx
cp /path/to/styx/editors/sublime-styx/* .
ln -s /path/to/styx/editors/shared/textmate/styx.tmLanguage.json .
```

For LSP support, install the [LSP package](https://packagecontrol.io/packages/LSP) and add:

```json
{
  "clients": {
    "styx": {
      "enabled": true,
      "command": ["styx", "@lsp"],
      "selector": "source.styx"
    }
  }
}
```

## Other Editors

Any editor with LSP support can use the Styx language server:

```bash
styx @lsp
```

The server communicates over stdio using the standard Language Server Protocol.

### What You Get

- **Diagnostics**: Syntax errors and schema validation
- **Hover**: Documentation for fields and types
- **Completion**: Autocomplete for keys, values, and tags
- **Formatting**: Consistent code style
