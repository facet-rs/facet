(comment) @comment

(string) @string
(escape_sequence) @string.escape

[
  "namespace"
  "entity"
  "action"
  "type"
  "in"
  "appliesTo"
  "attributes"
  "tags"
  "enum"
] @keyword

(integer) @number

(true) @boolean
(false) @boolean

[
  "principal"
  "resource"
  "context"
] @variable

[
  "Bool"
  "Long"
  "String"
  "Set"
] @type

"=" @operator

"?" @punctuation.special

[
  ","
  ";"
  ":"
  "::"
] @punctuation.delimiter

[
  "("
  ")"
  "{"
  "}"
  "["
  "]"
  "<"
  ">"
] @punctuation.bracket

(annotation
  "@" @punctuation.special
  (identifier) @attribute)

(namespace (name (identifier) @module))

(entity_declaration (identifier_list (identifier) @type))

(action_declaration (action_name_list (identifier) @function))

(common_type_declaration (identifier) @type.definition)

(attribute_declaration (identifier) @property)

(attribute_entry (identifier) @property)

(name (identifier) @type)

(qualified_name (identifier) @type)
