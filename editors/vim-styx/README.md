# Styx for Vim

Styx language support for classic Vim with regex-based syntax highlighting and LSP integration.

> **Note**: For Neovim users, see [nvim-styx](../nvim-styx/) which uses tree-sitter for more accurate highlighting.

## Installation

### Using vim-plug

```vim
Plug 'bearcove/styx', { 'rtp': 'editors/vim-styx' }
```

### Using Vundle

```vim
Plugin 'bearcove/styx', { 'rtp': 'editors/vim-styx' }
```

### Using pathogen

```bash
cd ~/.vim/bundle
git clone https://github.com/bearcove/styx
ln -s styx/editors/vim-styx vim-styx
```

### Manual Installation

Copy the contents to your Vim config:

```bash
cp -r editors/vim-styx/syntax ~/.vim/
cp -r editors/vim-styx/ftdetect ~/.vim/
cp -r editors/vim-styx/ftplugin ~/.vim/
```

## LSP Setup

The Styx CLI includes a built-in language server. You'll need an LSP client plugin.

### Requirements

Install the Styx CLI:

```bash
cargo install styx-cli
```

### Using coc.nvim (Recommended)

[coc.nvim](https://github.com/neoclide/coc.nvim) provides full LSP support including semantic tokens.

1. Install coc.nvim (requires Node.js):

```vim
Plug 'neoclide/coc.nvim', {'branch': 'release'}
```

2. Configure Styx LSP in `:CocConfig`:

```json
{
  "languageserver": {
    "styx": {
      "command": "styx",
      "args": ["lsp"],
      "filetypes": ["styx"],
      "rootPatterns": [".git"]
    }
  }
}
```

### Using vim-lsp

[vim-lsp](https://github.com/prabirshrestha/vim-lsp) is a lighter alternative that doesn't require Node.js.

1. Install vim-lsp:

```vim
Plug 'prabirshrestha/vim-lsp'
```

2. Configure Styx LSP in your `.vimrc`:

```vim
if executable('styx')
  au User lsp_setup call lsp#register_server({
    \ 'name': 'styx',
    \ 'cmd': {server_info->['styx', 'lsp']},
    \ 'allowlist': ['styx'],
    \ })
endif
```

## Features

- **Syntax highlighting** (regex-based, best-effort)
  - Comments (`//` and `///` doc comments)
  - Strings, raw strings, and heredocs
  - Numbers (decimal, hex, octal, binary, floats)
  - Booleans
  - Tags (`@name`)
  - Attributes (`key>`)
  - Directives (`@schema`, `@import`)

- **Filetype settings**
  - Comment string for `gc` motions (with commentary.vim, etc.)
  - 2-space indentation
  - Bracket matching

## Comparison with Neovim

| Feature | vim-styx | nvim-styx |
|---------|----------|-----------|
| Syntax highlighting | Regex-based | Tree-sitter |
| LSP support | Via plugin | Built-in |
| Semantic tokens | coc.nvim only | Native |
| Accuracy | Best-effort | Full grammar |

The regex-based syntax highlighting won't be as accurate as tree-sitter (e.g., it can't distinguish schema types from values), but it provides decent highlighting for quick editing and file viewing.
