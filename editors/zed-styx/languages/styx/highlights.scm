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

; Keys: first bare_scalar in an entry that has multiple values
(entry
  (value
    payload: (scalar (bare_scalar) @property))
  (value))

; Keys: first bare_scalar in single-value entry (implicit unit)
(entry
  .
  (value
    payload: (scalar (bare_scalar) @property))
  .)

; Scalars (general fallback)
(bare_scalar) @string
(quoted_scalar) @string
(raw_scalar) @string
(heredoc) @string

; Punctuation
"{" @punctuation.bracket
"}" @punctuation.bracket
"(" @punctuation.bracket
")" @punctuation.bracket
"," @punctuation.delimiter
