# Phase 012: vim-styx (Vim/Neovim Support)

Vim and Neovim support for Styx, with tree-sitter highlighting and LSP integration.

## Deliverables

- `editors/vim-styx/` - Vim plugin (traditional syntax)
- Tree-sitter integration for Neovim (via nvim-treesitter)
- LSP configuration examples

## Vim Plugin (Traditional)

For Vim users without tree-sitter:

### Package Structure

```
editors/vim-styx/
├── ftdetect/
│   └── styx.vim
├── ftplugin/
│   └── styx.vim
├── syntax/
│   └── styx.vim
├── indent/
│   └── styx.vim
└── README.md
```

### ftdetect/styx.vim

```vim
" Styx filetype detection
autocmd BufRead,BufNewFile *.styx setfiletype styx
```

### ftplugin/styx.vim

```vim
" Styx filetype settings
if exists('b:did_ftplugin')
  finish
endif
let b:did_ftplugin = 1

setlocal commentstring=//\ %s
setlocal comments=://
setlocal formatoptions-=t formatoptions+=croql

" Match pairs for %
setlocal matchpairs+={:},(:)

" Undo settings when switching filetypes
let b:undo_ftplugin = 'setlocal commentstring< comments< formatoptions< matchpairs<'
```

### syntax/styx.vim

```vim
" Styx syntax highlighting
if exists('b:current_syntax')
  finish
endif

" Comments
syn match styxLineComment "//.*$" contains=@Spell
syn match styxDocComment "///.*$" contains=@Spell

" Strings
syn region styxString start=/"/ skip=/\\./ end=/"/ contains=styxEscape
syn match styxEscape "\\." contained

" Raw strings
syn region styxRawString start=/r\z(#*\)"/ end=/"\z1/

" Heredoc
syn region styxHeredoc start=/<<\z([A-Z_][A-Z0-9_]*\)/ end=/^\z1$/

" Tags
syn match styxTag "@[a-zA-Z_][a-zA-Z0-9_]*"
syn match styxUnit "@\ze[^a-zA-Z_]"
syn match styxUnit "@$"

" Numbers (bare scalars that look numeric)
syn match styxNumber "\<-\?\d\+\(\.\d\+\)\?\>"

" Booleans
syn keyword styxBoolean true false

" Punctuation
syn match styxBraces "[{}()]"
syn match styxOperator "[=,]"

" Highlighting links
hi def link styxLineComment Comment
hi def link styxDocComment SpecialComment
hi def link styxString String
hi def link styxEscape SpecialChar
hi def link styxRawString String
hi def link styxHeredoc String
hi def link styxTag Type
hi def link styxUnit Constant
hi def link styxNumber Number
hi def link styxBoolean Boolean
hi def link styxBraces Delimiter
hi def link styxOperator Operator

let b:current_syntax = 'styx'
```

### indent/styx.vim

```vim
" Styx indentation
if exists('b:did_indent')
  finish
endif
let b:did_indent = 1

setlocal indentexpr=GetStyxIndent()
setlocal indentkeys=0{,0},0),!^F,o,O

function! GetStyxIndent()
  let lnum = prevnonblank(v:lnum - 1)
  if lnum == 0
    return 0
  endif
  
  let prev = getline(lnum)
  let curr = getline(v:lnum)
  let ind = indent(lnum)
  
  " Increase indent after { or (
  if prev =~ '[{(]\s*$'
    let ind += shiftwidth()
  endif
  
  " Decrease indent on } or )
  if curr =~ '^\s*[})]'
    let ind -= shiftwidth()
  endif
  
  return ind
endfunction
```

## Neovim Tree-sitter

For Neovim users with nvim-treesitter:

### Adding to nvim-treesitter

The tree-sitter-styx grammar needs to be registered with nvim-treesitter.

#### Option 1: Add to nvim-treesitter parsers

Submit PR to nvim-treesitter to add styx parser:

```lua
-- In nvim-treesitter/lua/nvim-treesitter/parsers.lua
styx = {
  install_info = {
    url = "https://github.com/bearcove/styx",
    location = "crates/tree-sitter-styx",
    files = { "src/parser.c" },
  },
  filetype = "styx",
  maintainers = { "@fasterthanlime" },
}
```

#### Option 2: Manual parser registration

In user's Neovim config:

```lua
local parser_config = require("nvim-treesitter.parsers").get_parser_configs()

parser_config.styx = {
  install_info = {
    url = "https://github.com/bearcove/styx",
    location = "crates/tree-sitter-styx",
    files = { "src/parser.c" },
    branch = "main",
  },
  filetype = "styx",
}

-- Filetype detection
vim.filetype.add({
  extension = {
    styx = "styx",
  },
})
```

### Highlight Queries

Create `queries/styx/highlights.scm`:

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

; Keys
(entry key: (scalar) @property)

; Punctuation
["{" "}" "(" ")"] @punctuation.bracket
["," "="] @punctuation.delimiter
"@" @punctuation.special

; Literals
((bare_scalar) @number
 (#match? @number "^-?[0-9]+(\\.[0-9]+)?$"))

((bare_scalar) @boolean
 (#any-of? @boolean "true" "false"))

(bare_scalar) @string
```

### Indent Queries

Create `queries/styx/indents.scm`:

```scheme
(object) @indent.begin
(sequence) @indent.begin

"}" @indent.end @indent.branch
")" @indent.end @indent.branch
```

### Fold Queries

Create `queries/styx/folds.scm`:

```scheme
(object) @fold
(sequence) @fold
(heredoc) @fold
```

## LSP Configuration

### Neovim (nvim-lspconfig)

Add styx-ls configuration:

```lua
-- In user's config or submit to nvim-lspconfig
local lspconfig = require('lspconfig')
local configs = require('lspconfig.configs')

if not configs.styx_ls then
  configs.styx_ls = {
    default_config = {
      cmd = { 'styx-ls' },
      filetypes = { 'styx' },
      root_dir = lspconfig.util.root_pattern('.git', '.styx-schema'),
      settings = {},
    },
    docs = {
      description = [[
Styx Language Server

https://github.com/bearcove/styx
]],
    },
  }
end

lspconfig.styx_ls.setup({})
```

### Vim (vim-lsp)

```vim
if executable('styx-ls')
  au User lsp_setup call lsp#register_server({
    \ 'name': 'styx-ls',
    \ 'cmd': {server_info->['styx-ls']},
    \ 'allowlist': ['styx'],
    \ })
endif
```

### Vim (coc.nvim)

In `coc-settings.json`:

```json
{
  "languageserver": {
    "styx": {
      "command": "styx-ls",
      "filetypes": ["styx"],
      "rootPatterns": [".git", ".styx-schema"]
    }
  }
}
```

## Installation

### Vim Plugin

```vim
" vim-plug
Plug 'bearcove/styx', { 'rtp': 'editors/vim-styx' }

" packer.nvim
use { 'bearcove/styx', rtp = 'editors/vim-styx' }

" lazy.nvim
{ 'bearcove/styx', config = function()
  vim.opt.rtp:append(vim.fn.stdpath('data') .. '/lazy/styx/editors/vim-styx')
end }
```

### Neovim Tree-sitter

```lua
-- After registering the parser
require('nvim-treesitter.configs').setup({
  ensure_installed = { 'styx' },
  highlight = { enable = true },
  indent = { enable = true },
})
```

## Testing

- Test syntax highlighting covers all constructs
- Test indentation works correctly
- Test LSP features with styx-ls
- Test folding works for objects/sequences
- Test comment toggling
