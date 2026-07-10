// The ratchet-facing Vix grammar. This is intentionally owned by the Vix
// compiler rather than by the legacy playground bundle: each accepted rung
// grows this grammar and its generated typed AST before it grows the checker.

const PREC = {
  compare: 1,
  add: 2,
  mul: 3,
  unary: 4,
  postfix: 5,
};

function sepBy(sep, rule) {
  return optional(seq(rule, repeat(seq(sep, rule)), optional(sep)));
}

module.exports = grammar({
  name: "vix_surface",

  extras: ($) => [/\s+/, $.line_comment],
  word: ($) => $.identifier,

  rules: {
    source_file: ($) => repeat(field("item", $._item)),

    _item: ($) => choice($.enum_item, $.struct_item, $.fn_item),

    attribute: ($) =>
      seq(
        "#",
        "[",
        field("name", $.identifier),
        optional(field("args", $.attribute_args)),
        "]",
      ),
    attribute_args: ($) => seq("{", sepBy(",", field("field", $.named_value)), "}"),

    fn_item: ($) =>
      seq(
        repeat(field("attribute", $.attribute)),
        optional(field("vis", "pub")),
        "fn",
        field("name", $.identifier),
        optional(field("generics", $.generic_params)),
        field("params", $.param_list),
        optional(field("where_params", $.where_params)),
        optional(seq("->", field("return_type", $._type))),
        field("body", $.block),
      ),

    struct_item: ($) =>
      seq(
        repeat(field("attribute", $.attribute)),
        optional(field("vis", "pub")),
        "struct",
        field("name", $.identifier),
        field("fields", $.record_field_list),
      ),
    record_field_list: ($) => seq("{", sepBy(",", field("field", $.record_field)), "}"),
    record_field: ($) => seq(field("name", $.identifier), ":", field("type", $._type)),
    enum_item: ($) =>
      seq(
        repeat(field("attribute", $.attribute)),
        optional(field("vis", "pub")),
        "enum",
        field("name", $.identifier),
        field("variants", $.enum_variant_list),
      ),
    enum_variant_list: ($) => seq("{", sepBy(",", field("variant", $.enum_variant)), "}"),
    enum_variant: ($) =>
      seq(
        repeat(field("attribute", $.attribute)),
        field("name", $.identifier),
        optional(field("payload", $._variant_type_payload)),
      ),
    _variant_type_payload: ($) => choice($.variant_tuple_type, $.record_field_list),
    variant_tuple_type: ($) => seq("(", sepBy(",", field("elem", $._type)), ")"),

    generic_params: ($) => seq("<", sepBy(",", field("param", $.identifier)), ">"),
    param_list: ($) => seq("(", sepBy(",", field("param", $.param)), ")"),
    param: ($) => seq(field("name", $.identifier), ":", field("type", $._type)),
    where_params: ($) =>
      seq(
        "where",
        choice(field("inline", $.named_param_list), field("named", $.type_path)),
      ),
    named_param_list: ($) => seq("{", sepBy(",", field("param", $.named_param)), "}"),
    named_param: ($) =>
      seq(
        field("name", $.identifier),
        ":",
        field("type", $._type),
        optional(seq("=", field("default", $._expr))),
      ),

    _type: ($) => choice($.generic_type, $.tuple_type, $.type_path),
    generic_type: ($) =>
      seq(field("base", $.type_path), "<", sepBy(",", field("arg", $._type)), ">"),
    tuple_type: ($) =>
      seq("(", field("elem", $._type), ",", sepBy(",", field("elem", $._type)), ")"),
    type_path: ($) =>
      seq(field("segment", $.identifier), repeat(seq("::", field("segment", $.identifier)))),

    block: ($) =>
      seq("{", repeat(field("stmt", $._statement)), optional(field("tail", $._expr)), "}"),
    _statement: ($) => choice($.let_statement, $.yield_statement, $.expression_statement),
    let_statement: ($) =>
      seq(
        "let",
        field("name", $.identifier),
        optional(seq(":", field("type", $._type))),
        "=",
        field("value", $._expr),
        ";",
      ),
    yield_statement: ($) => seq("yield", field("value", $._expr), ";"),
    expression_statement: ($) => seq(field("value", $._expr), ";"),

    _expr: ($) =>
      choice(
        $.match_expr,
        $.binary,
        $.unary,
        $.call,
        $.field_access,
        $.variant_expr,
        $.record_expr,
        $.tuple_expr,
        $.paren,
        $.identifier,
        $.string,
        $.number,
        $.boolean,
      ),

    binary: ($) => {
      const table = [
        [PREC.compare, choice("<=>", "==", "!=", "<", "<=", ">", ">=")],
        [PREC.add, choice("+", "-")],
        [PREC.mul, choice("*", "/")],
      ];
      return choice(
        ...table.map(([precedence, op]) =>
          prec.left(
            precedence,
            seq(field("left", $._expr), field("op", op), field("right", $._expr)),
          ),
        ),
      );
    },
    unary: ($) => prec(PREC.unary, seq(field("op", choice("-", "!")), field("value", $._expr))),
    call: ($) =>
      prec(
        PREC.postfix,
        seq(
          field("callee", $.identifier),
          field("args", $.arg_list),
          optional(field("named_args", $.where_args)),
        ),
      ),
    field_access: ($) =>
      prec(
        PREC.postfix,
        seq(field("receiver", $._expr), ".", field("name", choice($.identifier, $.tuple_index))),
      ),
    arg_list: ($) => seq("(", sepBy(",", field("arg", $._expr)), ")"),
    where_args: ($) => seq("where", "{", sepBy(",", field("field", $.named_value)), "}"),
    variant_path: ($) =>
      seq(field("type_name", $.identifier), "::", field("variant", $.identifier)),
    variant_expr: ($) =>
      prec(
        PREC.postfix,
        seq(
          field("path", $.variant_path),
          optional(field("tuple_payload", $.arg_list)),
        ),
      ),
    record_expr: ($) =>
      prec(PREC.postfix, seq(field("type", $.type_path), field("fields", $.record_value_list))),
    record_value_list: ($) =>
      seq(
        "{",
        optional(
          choice(
            seq(
              field("spread", $.record_spread),
              optional(seq(",", sepBy(",", field("field", $.named_value)))),
            ),
            seq(
              field("field", $.named_value),
              repeat(seq(",", field("field", $.named_value))),
              optional(seq(",", field("spread", $.record_spread))),
              optional(","),
            ),
          ),
        ),
        "}",
      ),
    record_spread: ($) => seq("..", field("base", $._expr)),
    named_value: ($) =>
      seq(
        field("name", $.identifier),
        optional(seq(":", field("value", $._expr))),
      ),
    tuple_expr: ($) =>
      seq("(", field("elem", $._expr), ",", sepBy(",", field("elem", $._expr)), ")"),
    paren: ($) => seq("(", field("inner", $._expr), ")"),

    match_expr: ($) =>
      seq("match", field("scrutinee", $._expr), field("arms", $.match_arm_list)),
    match_arm_list: ($) => seq("{", sepBy(",", field("arm", $.match_arm)), "}"),
    match_arm: ($) =>
      seq(field("pattern", $._pattern), "=>", field("body", $._expr)),
    _pattern: ($) => choice($.variant_pattern),
    variant_pattern: ($) =>
      seq(
        field("path", $.variant_path),
        optional(field("payload", $._variant_pattern_payload)),
      ),
    _variant_pattern_payload: ($) => choice($.tuple_pattern, $.record_pattern),
    tuple_pattern: ($) => seq("(", sepBy(",", field("binding", $.identifier)), ")"),
    record_pattern: ($) => seq("{", sepBy(",", field("field", $.pattern_field)), "}"),
    pattern_field: ($) =>
      seq(
        field("name", $.identifier),
        optional(seq(":", field("binding", $.identifier))),
      ),

    identifier: () => /[A-Za-z_][A-Za-z0-9_]*/,
    string: () => /"([^"\\]|\\.)*"/,
    number: () => /[0-9]+/,
    tuple_index: () => /[0-9]+/,
    boolean: () => choice("true", "false"),
    line_comment: () => token(seq("//", /[^\n]*/)),
  },
});
