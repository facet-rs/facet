(comment) @comment

(string) @string
(escape_sequence) @string.escape

(integer) @number

(true) @boolean
(false) @boolean

[
  "permit"
  "forbid"
  "when"
  "unless"
  "template"
] @keyword

[
  "if"
  "then"
  "else"
] @keyword.conditional

[
  "in"
  "has"
  "like"
  "is"
] @keyword.operator

[
  "principal"
  "action"
  "resource"
  "context"
] @variable

(slot
  "?" @variable.parameter
  (identifier) @variable.parameter)

[
  "=="
  "!="
  "<"
  "<="
  ">"
  ">="
  "=>"
  "&&"
  "||"
  "+"
  "-"
  "*"
  "/"
  "%"
  "!"
  "="
  "&"
  "|"
] @operator

[
  ","
  ";"
  "."
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
] @punctuation.bracket

(annotation
  "@" @punctuation.special
  (identifier) @attribute)

((extension_call
  (name (identifier) @function))
  (#any-of? @function "ip" "decimal" "datetime" "duration"))

((method_call
  (identifier) @function.method)
  (#any-of? @function.method
    "contains" "containsAll" "containsAny"
    "isEmpty"
    "getTag" "hasTag"
    "isIpv4" "isIpv6" "isLoopback" "isMulticast" "isInRange"
    "lessThan" "lessThanOrEqual" "greaterThan" "greaterThanOrEqual"
    "offset" "durationSince" "toDate" "toTime"
    "toMilliseconds" "toSeconds" "toMinutes" "toHours" "toDays"))

(field_access (identifier) @property)
(record_entry (identifier) @property)
(ref_init (identifier) @property)

(entity_reference (identifier) @type)
(type_reference (name (identifier) @type))
(scope_constraint "is" (name (identifier) @type))
(relation_expression "is" (name (identifier) @type))
