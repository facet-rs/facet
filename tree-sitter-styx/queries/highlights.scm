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
; Use @keyword or @punctuation.special to make > stand out
(attribute
  key: (bare_scalar) @property
  ">" @keyword)

; Keys in entries - any scalar in the key position (overrides @string above)
(entry
  key: (expr
    payload: (scalar) @property))

; Sequence items are values, not keys (must come AFTER entry key rule to override)
(sequence
  (expr
    payload: (scalar (bare_scalar) @string)))

; Punctuation
"{" @punctuation.bracket
"}" @punctuation.bracket
"(" @punctuation.bracket
")" @punctuation.bracket
"," @punctuation.delimiter
