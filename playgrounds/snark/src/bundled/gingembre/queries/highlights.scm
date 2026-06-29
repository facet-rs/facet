; gingembre (Jinja-like) highlights

(comment) @comment

; delimiters
[
  "{{"
  "}}"
  "{{-"
  "-}}"
  "{%"
  "%}"
  "{%-"
  "-%}"
] @punctuation.special

; keywords
[
  "if"
  "elif"
  "else"
  "endif"
  "for"
  "in"
  "endfor"
  "set"
  "endset"
  "block"
  "endblock"
  "macro"
  "endmacro"
  "extends"
  "include"
  "import"
  "break"
  "continue"
  "as"
  "and"
  "or"
  "not"
  "is"
] @keyword

; operators
[
  "+"
  "-"
  "*"
  "/"
  "//"
  "%"
  "**"
  "~"
  "=="
  "!="
  "<"
  ">"
  "<="
  ">="
  "|"
  "="
  "::"
  "?"
] @operator

; brackets / punctuation
[
  "("
  ")"
  "["
  "]"
  "{"
  "}"
] @punctuation.bracket

[
  ","
  ":"
  "."
] @punctuation.delimiter

; literals
(number) @number
(string) @string
(boolean) @constant.builtin
(none) @constant.builtin

; names
(variable (identifier) @variable)
(field (identifier) @property)
(kwarg (identifier) @property)
(param (identifier) @variable)
(for_statement (identifier) @variable)
(set_statement (identifier) @variable)

; calls, filters, tests, macros
(filter (identifier) @function)
(test (identifier) @function)
(macro_call (identifier) @variable (identifier) @function)
(macro_statement (identifier) @function)
(block_statement (identifier) @function)
