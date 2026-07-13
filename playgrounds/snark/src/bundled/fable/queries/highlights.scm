; fable highlights

(line_comment) @comment
(block_comment) @comment

[
  "if"
  "else"
  "let"
  "struct"
  "enum"
  "match"
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

(struct_decl
  name: (type_identifier) @type)

(enum_decl
  name: (type_identifier) @type)

(enum_variant_decl
  name: (type_identifier) @constructor)

(variant_path
  type_name: (type_identifier) @type
  variant_name: (type_identifier) @constructor)

(struct_field
  name: (_) @property)

(type_field
  name: (_) @property)

(pattern_field
  name: (_) @property)

(wildcard_pattern) @constant.builtin

[
  "=>"
  "::"
] @punctuation.delimiter

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
