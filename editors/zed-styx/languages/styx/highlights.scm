; Styx syntax highlighting queries

; Comments
(line_comment) @comment
(doc_comment) @comment.documentation

; Escape sequences in quoted strings
(escape_sequence) @string.escape

; Unit value
(unit) @constant.builtin

; Tags
(tag) @type

; Attributes
(attribute
  key: (bare_scalar) @property
  "=" @operator)

; Scalars (general fallback) - must come BEFORE more specific rules
(bare_scalar) @string
(quoted_scalar) @string
(raw_scalar) @string
(heredoc) @string

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
