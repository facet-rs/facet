; Styx syntax highlighting queries

; Comments
(line_comment) @comment
(doc_comment) @comment.documentation

; Keys: first value in an entry (when it's a bare scalar)
(entry
  .
  (value
    payload: (scalar
      (bare_scalar) @property)))

; Scalars (general - lower priority than keys above)
(bare_scalar) @string
(quoted_scalar) @string
(raw_scalar) @string
(heredoc) @string

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

; Punctuation
"{" @punctuation.bracket
"}" @punctuation.bracket
"(" @punctuation.bracket
")" @punctuation.bracket
"," @punctuation.delimiter
