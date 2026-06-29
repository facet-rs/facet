; GraphQL highlights query

; Comments
(comment) @comment

; Strings
(string_value) @string
(description) @string.documentation

; Numbers
(int_value) @number
(float_value) @number

; Boolean and null values
(boolean_value) @constant.builtin
(null_value) @constant.builtin

; Enum values
(enum_value) @constant

; Type definitions
(object_type_definition
  (name) @type.definition)

(interface_type_definition
  (name) @type.definition)

(input_object_type_definition
  (name) @type.definition)

(enum_type_definition
  (name) @type.definition)

(scalar_type_definition
  (name) @type.definition)

(union_type_definition
  (name) @type.definition)

; Type references
(named_type
  (name) @type)

; Field definitions
(field_definition
  (name) @property)

; Input value definitions (arguments in schema)
(input_value_definition
  (name) @variable.parameter)

; Operations
(operation_definition
  (name) @function)

(operation_type) @keyword

; Fragments
(fragment_definition
  (fragment_name) @function)

(fragment_spread
  (fragment_name) @function)

; Fields in queries
(field
  (name) @property)

(field
  (alias
    (name) @property))

; Arguments
(argument
  (name) @variable.parameter)

; Object fields (in values)
(object_field
  (name) @property)

; Directives
(directive
  "@" @punctuation.special
  (name) @attribute)

(directive_definition
  (name) @attribute)

; Keywords
[
  "query"
  "mutation"
  "subscription"
  "fragment"
  "on"
  "type"
  "interface"
  "implements"
  "input"
  "enum"
  "union"
  "scalar"
  "extend"
  "schema"
  "directive"
  "repeatable"
] @keyword

; Operators
[
  "="
  "!"
  "|"
  "&"
  "..."
] @operator

; Punctuation
[
  "("
  ")"
  "["
  "]"
  "{"
  "}"
] @punctuation.bracket

":" @punctuation.delimiter

; Variables
(variable
  "$" @punctuation.special
  (name) @variable)

; Name fallback
(name) @variable
