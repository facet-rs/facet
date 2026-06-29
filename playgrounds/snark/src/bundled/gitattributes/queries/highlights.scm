(dir_sep) @punctuation.delimiter

(quoted_pattern
  "\"" @punctuation.special)

(range_notation) @string.special

(range_notation
  [ "[" "]" ] @punctuation.bracket)

(wildcard) @string.regexp

(range_negation) @operator

(character_class) @constant

(class_range "-" @operator)

[
  (ansi_c_escape)
  (escaped_char)
] @escape

(attribute
  (attr_name) @variable.parameter)

(attribute
  (builtin_attr) @variable.builtin)

[
  (attr_reset)
  (attr_unset)
  (attr_set)
] @operator

(boolean_value) @boolean

(string_value) @string

(macro_tag) @keyword

(macro_def
  macro_name: (_) @property)

[
  (pattern_negation)
  (redundant_escape)
  (trailing_slash)
  (ignored_value)
] @error

(comment) @comment
