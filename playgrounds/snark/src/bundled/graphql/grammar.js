module.exports = grammar({
  name: "graphql",

  extras: ($) => [/[\s\uFEFF\u0009\u0020\u000A\u000D]/, $.comma, $.comment],

  rules: {
    source_file: ($) => $.document,
    document: ($) => repeat1($.definition),
    definition: ($) =>
      choice(
        $.executable_definition,
        $.type_system_definition,
        $.type_system_extension
      ),
    executable_definition: ($) =>
      choice($.operation_definition, $.fragment_definition),
    type_system_definition: ($) =>
      choice($.schema_definition, $.type_definition, $.directive_definition),
    type_system_extension: ($) => choice($.schema_extension, $.type_extension),
    schema_definition: ($) =>
      seq(
        optional($.description),
        "schema",
        optional($.directives),
        "{",
        repeat1($.root_operation_type_definition),
        "}"
      ),
    schema_extension: ($) =>
      seq(
        "extend",
        "schema",
        optional($.directives),
        "{",
        $.root_operation_type_definition,
        "}"
      ),
    type_extension: ($) =>
      choice(
        $.scalar_type_extension,
        $.object_type_extension,
        $.interface_type_extension,
        $.union_type_extension,
        $.enum_type_extension,
        $.input_object_type_extension
      ),
    scalar_type_extension: ($) => seq("extend", "scalar", $.name, $.directives),
    object_type_extension: ($) =>
      prec.right(
        choice(
          seq(
            "extend",
            "type",
            $.name,
            optional($.implements_interfaces),
            optional($.directives),
            $.fields_definition
          ),
          seq(
            "extend",
            "type",
            $.name,
            optional($.implements_interfaces),
            optional($.directives)
          )
        )
      ),
    interface_type_extension: ($) =>
      prec.right(
        choice(
          seq(
            "extend",
            "interface",
            $.name,
            optional($.implements_interfaces),
            optional($.directives),
            $.fields_definition
          ),
          seq(
            "extend",
            "interface",
            $.name,
            optional($.implements_interfaces),
            optional($.directives)
          )
        )
      ),
    union_type_extension: ($) =>
      prec.right(
        choice(
          seq(
            "extend",
            "union",
            $.name,
            optional($.directives),
            $.union_member_types
          ),
          seq("extend", "union", $.name, optional($.directives))
        )
      ),
    enum_type_extension: ($) =>
      prec.right(
        choice(
          seq(
            "extend",
            "enum",
            $.name,
            optional($.directives),
            $.enum_values_definition
          ),
          seq("extend", "enum", $.name, optional($.directives))
        )
      ),
    input_object_type_extension: ($) =>
      prec.right(
        choice(
          seq(
            "extend",
            "input",
            $.name,
            optional($.directives),
            repeat1($.input_fields_definition)
          ),
          seq("extend", "input", $.name, optional($.directives))
        )
      ),
    input_fields_definition: ($) =>
      seq("{", repeat1($.input_value_definition), "}"),
    enum_values_definition: ($) =>
      seq("{", repeat1($.enum_value_definition), "}"),
    enum_value_definition: ($) =>
      seq(optional($.description), $.enum_value, optional($.directives)),
    implements_interfaces: ($) =>
      choice(
        seq($.implements_interfaces, "&", $.named_type),
        seq("implements", optional("&"), $.named_type)
      ),
    fields_definition: ($) => seq("{", repeat1($.field_definition), "}"),
    field_definition: ($) =>
      seq(
        optional($.description),
        $.name,
        optional($.arguments_definition),
        ":",
        $.type,
        optional($.directives)
      ),
    arguments_definition: ($) =>
      seq("(", repeat1($.input_value_definition), ")"),
    input_value_definition: ($) =>
      seq(
        optional($.description),
        $.name,
        ":",
        $.type,
        optional($.default_value),
        optional($.directives)
      ),
    default_value: ($) => seq("=", $.value),
    union_member_types: ($) =>
      choice(
        seq($.union_member_types, "|", $.named_type),
        seq("=", optional("|"), $.named_type)
      ),
    root_operation_type_definition: ($) =>
      seq($.operation_type, ":", $.named_type),
    operation_definition: ($) =>
      choice(
        $.selection_set,
        seq(
          $.operation_type,
          optional($.name),
          optional($.variable_definitions),
          optional($.directives),
          $.selection_set
        )
      ),
    operation_type: ($) => choice("query", "mutation", "subscription"),
    type_definition: ($) =>
      choice(
        $.scalar_type_definition,
        $.object_type_definition,
        $.interface_type_definition,
        $.union_type_definition,
        $.enum_type_definition,
        $.input_object_type_definition
      ),
    scalar_type_definition: ($) =>
      prec.right(
        seq(optional($.description), "scalar", $.name, optional($.directives))
      ),
    object_type_definition: ($) =>
      prec.right(
        seq(
          optional($.description),
          "type",
          $.name,
          optional($.implements_interfaces),
          optional($.directives),
          optional($.fields_definition)
        )
      ),
    interface_type_definition: ($) =>
      prec.right(
        seq(
          optional($.description),
          "interface",
          $.name,
          optional($.implements_interfaces),
          optional($.directives),
          optional($.fields_definition)
        )
      ),
    union_type_definition: ($) =>
      prec.right(
        seq(
          optional($.description),
          "union",
          $.name,
          optional($.directives),
          optional($.union_member_types)
        )
      ),
    enum_type_definition: ($) =>
      prec.right(
        seq(
          optional($.description),
          "enum",
          $.name,
          optional($.directives),
          optional($.enum_values_definition)
        )
      ),
    input_object_type_definition: ($) =>
      prec.right(
        seq(
          optional($.description),
          "input",
          $.name,
          optional($.directives),
          optional($.input_fields_definition)
        )
      ),
    variable_definitions: ($) => seq("(", repeat1($.variable_definition), ")"),
    variable_definition: ($) =>
      seq(
        $.variable,
        ":",
        $.type,
        optional($.default_value),
        optional($.directives),
        optional($.comma)
      ),
    selection_set: ($) => seq("{", repeat1($.selection), "}"),
    selection: ($) => choice($.field, $.inline_fragment, $.fragment_spread),
    field: ($) =>
      seq(
        optional($.alias),
        $.name,
        optional($.arguments),
        optional($.directive),
        optional($.selection_set)
      ),
    alias: ($) => seq($.name, ":"),
    arguments: ($) => seq("(", repeat1($.argument), ")"),
    argument: ($) => seq($.name, ":", $.value),
    value: ($) =>
      choice(
        $.variable,
        $.string_value,
        $.int_value,
        $.float_value,
        $.boolean_value,
        $.null_value,
        $.enum_value,
        $.list_value,
        $.object_value
      ),
    variable: ($) => seq("$", $.name),
    string_value: ($) =>
      choice(
        seq('"""', /([^"]|\n|""?[^"])*/, '"""'),
        seq('"', /[^"\\\n]*/, '"')
      ),
    int_value: ($) => /-?(0|[1-9][0-9]*)/,
    float_value: ($) =>
      token(
        seq(
          /-?(0|[1-9][0-9]*)/,
          choice(
            /\.[0-9]+/,
            /(e|E)(\+|-)?[0-9]+/,
            seq(/\.[0-9]+/, /(e|E)(\+|-)?[0-9]+/)
          )
        )
      ),
    boolean_value: ($) => choice("true", "false"),
    null_value: ($) => "null",
    enum_value: ($) => $.name,
    list_value: ($) => seq("[", repeat($.value), "]"),
    object_value: ($) => seq("{", repeat($.object_field), "}"),
    object_field: ($) => seq($.name, ":", $.value, optional($.comma)),
    fragment_spread: ($) => seq("...", $.fragment_name, optional($.directives)),
    fragment_definition: ($) =>
      seq(
        "fragment",
        $.fragment_name,
        $.type_condition,
        optional($.directives),
        $.selection_set
      ),
    fragment_name: ($) => $.name,
    inline_fragment: ($) =>
      seq(
        "...",
        optional($.type_condition),
        optional($.directives),
        $.selection_set
      ),
    type_condition: ($) => seq("on", $.named_type),
    directives: ($) => repeat1($.directive),
    directive: ($) => seq("@", $.name, optional($.arguments)),
    directive_definition: ($) =>
      prec.right(
        1,
        seq(
          optional($.description),
          "directive",
          "@",
          $.name,
          optional($.arguments_definition),
          optional("repeatable"),
          "on",
          $.directive_locations
        )
      ),
    directive_locations: ($) =>
      choice(
        seq($.directive_locations, "|", $.directive_location),
        seq(optional("|"), $.directive_location)
      ),
    directive_location: ($) =>
      choice($.executable_directive_location, $.type_system_directive_location),
    executable_directive_location: ($) =>
      choice(
        "QUERY",
        "MUTATION",
        "SUBSCRIPTION",
        "FIELD",
        "FRAGMENT_DEFINITION",
        "FRAGMENT_SPREAD",
        "INLINE_FRAGMENT",
        "VARIABLE_DEFINITION"
      ),
    type_system_directive_location: ($) =>
      choice(
        "SCHEMA",
        "SCALAR",
        "OBJECT",
        "FIELD_DEFINITION",
        "ARGUMENT_DEFINITION",
        "INTERFACE",
        "UNION",
        "ENUM",
        "ENUM_VALUE",
        "INPUT_OBJECT",
        "INPUT_FIELD_DEFINITION"
      ),
    type: ($) => choice($.named_type, $.list_type, $.non_null_type),
    named_type: ($) => $.name,
    list_type: ($) => seq("[", $.type, "]"),
    non_null_type: ($) => choice(seq($.named_type, "!"), seq($.list_type, "!")),
    name: ($) => /[_A-Za-z][_0-9A-Za-z]*/,
    comment: ($) => token(seq("#", /.*/)),
    comma: ($) => ",",
    description: ($) => $.string_value,
  },
});
