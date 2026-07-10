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

    _item: ($) => choice($.fn_item),

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
    _statement: ($) => choice($.let_statement, $.yield_statement),
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

    _expr: ($) =>
      choice(
        $.binary,
        $.unary,
        $.call,
        $.tuple_expr,
        $.paren,
        $.identifier,
        $.string,
        $.number,
        $.boolean,
      ),

    binary: ($) => {
      const table = [
        [PREC.compare, choice("==", "!=", "<", "<=", ">", ">=")],
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
    arg_list: ($) => seq("(", sepBy(",", field("arg", $._expr)), ")"),
    where_args: ($) => seq("where", "{", sepBy(",", field("field", $.named_value)), "}"),
    named_value: ($) =>
      seq(
        field("name", $.identifier),
        optional(seq(":", field("value", $._expr))),
      ),
    tuple_expr: ($) =>
      seq("(", field("elem", $._expr), ",", sepBy(",", field("elem", $._expr)), ")"),
    paren: ($) => seq("(", field("inner", $._expr), ")"),

    identifier: () => /[A-Za-z_][A-Za-z0-9_]*/,
    string: () => /"([^"\\]|\\.)*"/,
    number: () => /[0-9]+/,
    boolean: () => choice("true", "false"),
    line_comment: () => token(seq("//", /[^\n]*/)),
  },
});
