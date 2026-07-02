; vix v0 highlights

(line_comment) @comment
(doc_comment) @comment.documentation

[
  "fn"
  "let"
  "use"
  "pub"
  "match"
] @keyword

(boolean) @constant.builtin
(number) @number
(string) @string
(path_literal) @string.special.path
(flag) @constant

; declarations & references
(fn_item name: (identifier) @function)
(call callee: (identifier) @function.call)
(call callee: (scoped_identifier (identifier) @function.call .))
(method_call name: (identifier) @function.method)
(param name: (identifier) @variable.parameter)
(field_access name: (identifier) @property)
(kwarg name: (identifier) @variable.parameter)

; types
(type_path (identifier) @type)
(array_type) @type

; command blocks: the command name pops, the soup stays muted
(command_block command: (identifier) @function.macro)
(command_token) @string.special
(splice ["{" "}"] @punctuation.special)

[
  "->"
  "=>"
  "::"
] @punctuation.delimiter

[
  "=="
  "!="
  "<"
  "<="
  ">"
  ">="
  "&&"
  "||"
  "/"
  "+"
  "-"
  "*"
  "%"
  "="
  "!"
] @operator

(wildcard_pattern) @constant.builtin
