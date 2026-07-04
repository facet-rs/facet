; fable highlights

(line_comment) @comment
(block_comment) @comment

[
  "if"
  "else"
  "let"
  "and"
  "or"
  "not"
] @keyword

[
  (true_literal)
  (false_literal)
  (null_literal)
] @constant.builtin

(int_literal) @number
(float_literal) @number
(string_literal) @string

(call_expr
  callee: (var_ref
    name: (_) @function.call))

(field_expr
  field_name: (_) @property)

(struct_literal
  type_name: (type_identifier) @type)

(struct_field
  name: (_) @property)

[
  "=="
  "!="
  "<"
  ">"
  "<="
  ">="
  "+"
  "-"
  "="
] @operator
