" Vim syntax file
" Language: Styx
" Maintainer: bearcove
" Latest Revision: 2025

if exists("b:current_syntax")
  finish
endif

" Comments
syn match styxComment "//.*$" contains=styxTodo
syn match styxDocComment "///.*$" contains=styxTodo
syn keyword styxTodo contained TODO FIXME XXX NOTE HACK BUG

" Numbers
syn match styxNumber "\v<-?\d+>"
syn match styxNumber "\v<-?\d+\.\d+>"
syn match styxNumber "\v<-?\d+[eE][+-]?\d+>"
syn match styxNumber "\v<-?\d+\.\d+[eE][+-]?\d+>"
syn match styxNumber "\v<0x[0-9a-fA-F]+>"
syn match styxNumber "\v<0o[0-7]+>"
syn match styxNumber "\v<0b[01]+>"

" Booleans
syn keyword styxBoolean true false

" Tags (@ followed by identifier or just @)
syn match styxTag "@[a-zA-Z_][a-zA-Z0-9_-]*"
syn match styxUnit "@"

" Strings
syn region styxString start=+"+ skip=+\\\\\|\\"+ end=+"+ contains=styxEscape
syn match styxEscape contained "\\[nrtfb\\\"0]"
syn match styxEscape contained "\\x[0-9a-fA-F]\{2}"
syn match styxEscape contained "\\u{[0-9a-fA-F]\{1,6}}"

" Raw strings (backtick-delimited)
syn region styxRawString start=+`+ end=+`+

" Heredocs
syn region styxHeredoc start="<<[A-Z_][A-Z0-9_]*" end="^[A-Z_][A-Z0-9_]*$" contains=styxHeredocDelim
syn match styxHeredocDelim contained "<<[A-Z_][A-Z0-9_]*"
syn match styxHeredocDelim contained "^[A-Z_][A-Z0-9_]*$"

" Heredoc with language hint (e.g., <<SQL,sql)
syn region styxHeredocLang start="<<[A-Z_][A-Z0-9_]*,[a-z]\+" end="^[A-Z_][A-Z0-9_]*$" contains=styxHeredocDelim,styxHeredocLangHint
syn match styxHeredocLangHint contained ",[a-z]\+"

" Attributes (key> value)
syn match styxAttribute "[a-zA-Z_][a-zA-Z0-9_-]*>" contains=styxAttributeKey,styxAttributeArrow
syn match styxAttributeKey contained "[a-zA-Z_][a-zA-Z0-9_-]*"
syn match styxAttributeArrow contained ">"

" Schema directive
syn match styxDirective "@schema"
syn match styxDirective "@import"

" Punctuation
syn match styxBrace "[{}()]"
syn match styxDelimiter ","

" Define the default highlighting
hi def link styxComment Comment
hi def link styxDocComment SpecialComment
hi def link styxTodo Todo
hi def link styxNumber Number
hi def link styxBoolean Boolean
hi def link styxTag Type
hi def link styxUnit Constant
hi def link styxString String
hi def link styxRawString String
hi def link styxEscape SpecialChar
hi def link styxHeredoc String
hi def link styxHeredocDelim Delimiter
hi def link styxHeredocLangHint Label
hi def link styxAttribute Identifier
hi def link styxAttributeKey Identifier
hi def link styxAttributeArrow Operator
hi def link styxDirective PreProc
hi def link styxBrace Delimiter
hi def link styxDelimiter Delimiter

let b:current_syntax = "styx"
