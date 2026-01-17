; Styx syntax highlighting queries for Neovim

; Comments
(line_comment) @comment
(doc_comment) @comment.documentation

; Escape sequences in quoted strings
(escape_sequence) @string.escape

; Scalars (general fallback)
(bare_scalar) @string
(quoted_scalar) @string
(raw_scalar) @string
(heredoc) @string

; Unit value
(unit) @constant.builtin

; Tags
(tag) @label

; Attributes - key in attribute syntax
(attribute
  key: (bare_scalar) @property
  ">" @keyword)

; Keys in entries
(entry
  key: (expr
    payload: (scalar (bare_scalar) @property)))

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
