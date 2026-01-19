# Styx for Neovim

Styx language support for Neovim with tree-sitter and LSP.

## Installation

### Using lazy.nvim

```lua
{
  "bearcove/styx",
  config = function()
    -- Add queries to runtimepath
    vim.opt.runtimepath:append(vim.fn.stdpath("data") .. "/lazy/styx/editors/nvim-styx")
  end,
}
```

### Manual Installation

Copy or symlink the contents of `editors/nvim-styx` to your Neovim config:

```bash
# Symlink approach
ln -s /path/to/styx/editors/nvim-styx/queries ~/.config/nvim/queries
ln -s /path/to/styx/editors/nvim-styx/ftdetect ~/.config/nvim/ftdetect
ln -s /path/to/styx/editors/nvim-styx/ftplugin ~/.config/nvim/ftplugin
```

## Tree-sitter Setup

Add the Styx parser to nvim-treesitter:

```lua
local parser_config = require("nvim-treesitter.parsers").get_parser_configs()
parser_config.styx = {
  install_info = {
    url = "https://github.com/bearcove/styx",
    files = { "crates/tree-sitter-styx/src/parser.c", "crates/tree-sitter-styx/src/scanner.c" },
    branch = "main",
    location = "crates/tree-sitter-styx",
  },
  filetype = "styx",
}
```

Then install the parser:

```vim
:TSInstall styx
```

## LSP Setup

### Using nvim-lspconfig

Add to your LSP configuration:

```lua
local lspconfig = require("lspconfig")
local configs = require("lspconfig.configs")

-- Register Styx LSP if not already defined
if not configs.styx then
  configs.styx = {
    default_config = {
      cmd = { "styx", "lsp" },
      filetypes = { "styx" },
      root_dir = lspconfig.util.root_pattern(".git", "*.styx"),
      settings = {},
    },
  }
end

lspconfig.styx.setup({})
```

### Requirements

The Styx CLI must be installed:

```bash
cargo install styx-cli
```

## Features

- Syntax highlighting via tree-sitter
- LSP integration (diagnostics, hover, completions)
- Filetype detection for `.styx` files
- Comment string configuration for `gc` motions
