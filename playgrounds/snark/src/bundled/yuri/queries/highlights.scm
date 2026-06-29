; Identifiers
(primitive_type) @type.builtin

((identifier) @constant
  (#match? @constant "^[A-Z][A-Z\\d_]+$'"))

((identifier) @type
  (#match? @type "^[A-Z]"))

((symbol) @type
  (#match? @type "^[A-Z]"))

(type_alias_item
  name: (symbol) @type)

; Function calls
(call_expression
  function: (identifier) @function)

; Function definitions
(function_item
  (symbol) @function)

(variable_item
  (symbol) @variable)

(line_comment) @comment

[
  "("
  ")"
  "{"
  "}"
  "["
  "]"
  "{{"
  "}}"
] @punctuation.bracket

"." @punctuation.delimiter

"," @punctuation.delimiter

":" @punctuation.delimiter

(attribute_item) @attribute

; Keywords
[
  ; declaration
  "import"
  "export"
  "fn"
  "let"
  "type"
  "module"
  ; control flow
  "break"
  "continue"
  "else"
  "if"
  "return"
  ; array processors
  ; 	"loop"
  ; 	"reverse"
  ; 	"fold"
  ; 	"map"
  ; 	"filter"
  ; 	"append"
  ; 	"prepend"
  ; 	"join"
  ; 	"flatten"
] @keyword

(boolean_literal) @boolean

(integer_literal) @number

(float_literal) @number

[
  "~"
  "!"
  "and"
  "or"
  "xor"
  "&"
  "^"
  "|"
  "=="
  "!="
  "<"
  "<="
  ">"
  ">="
  "<<"
  ">>"
  "+"
  "-"
  "*"
  "/"
  "%"
  "**"
] @operator
