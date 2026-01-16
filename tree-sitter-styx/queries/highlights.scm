; Styx syntax highlighting queries

; Comments
(line_comment) @comment
(doc_comment) @comment.documentation

; Escape sequences in quoted strings
(escape_sequence) @string.escape

; Scalars (general fallback) - must come BEFORE more specific rules
; With tree-sitter convention, later patterns override earlier ones
(bare_scalar) @string
(quoted_scalar) @string
(raw_scalar) @string
(heredoc) @string

; Unit value
(unit) @constant.builtin

; Tags - more specific than bare_scalar, so comes after
(tag) @label

; Attributes - key in attribute syntax
(attribute
  key: (bare_scalar) @property
  "=" @operator)

; Keys in entries - bare scalars in the key position (overrides @string above)
(entry
  key: (expr
    payload: (scalar (bare_scalar) @property)))

; Punctuation
"{" @punctuation.bracket
"}" @punctuation.bracket
"(" @punctuation.bracket
")" @punctuation.bracket
"," @punctuation.delimiter
