; Styx syntax highlighting queries

; Comments
(line_comment) @comment
(doc_comment) @comment.documentation

; Scalars
(bare_scalar) @string
(quoted_scalar) @string
(raw_scalar) @string
(heredoc) @string

; Escape sequences in quoted strings
(escape_sequence) @string.escape

; Unit value
(unit) @constant.builtin

; Tags (the whole tag including @name is captured by the external scanner)
(tag) @type

; Attributes
(attribute
  key: (bare_scalar) @property
  "=" @operator)

; Punctuation
"{" @punctuation.bracket
"}" @punctuation.bracket
"(" @punctuation.bracket
")" @punctuation.bracket
"," @punctuation.delimiter
