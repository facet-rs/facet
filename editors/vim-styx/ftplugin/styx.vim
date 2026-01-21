" Filetype plugin for Styx files
if exists("b:did_ftplugin")
  finish
endif
let b:did_ftplugin = 1

" Comment settings for commentary.vim, tpope/vim-commentary, etc.
setlocal commentstring=//\ %s
setlocal comments=:///,://

" Indentation
setlocal expandtab
setlocal shiftwidth=2
setlocal softtabstop=2
setlocal tabstop=2

" Format options
setlocal formatoptions-=t  " Don't auto-wrap text
setlocal formatoptions+=c  " Auto-wrap comments
setlocal formatoptions+=r  " Continue comments on Enter
setlocal formatoptions+=o  " Continue comments with o/O
setlocal formatoptions+=q  " Allow formatting comments with gq
setlocal formatoptions+=l  " Don't break long lines in insert mode

" Match pairs
setlocal matchpairs+={:},(:)

" Undo ftplugin settings when switching filetype
let b:undo_ftplugin = "setlocal commentstring< comments< expandtab< shiftwidth< softtabstop< tabstop< formatoptions< matchpairs<"
