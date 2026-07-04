// Snark grammar for fable: the tiny typed language over Facet-reflected values.
//
// Mirrors fable/src/{lexer,parser}.rs while the migration runs. The grammar is
// intentionally syntax-only: no declarations, functions, or new language teeth.

const PREC = {
  or: 1,
  and: 2,
  not: 3,
  compare: 4,
  add: 5,
  unary: 6,
  postfix: 7,
};

function sepBy(sep, rule) {
  return optional(seq(rule, repeat(seq(sep, rule)), optional(sep)));
}

module.exports = grammar({
  name: "fable",

  extras: ($) => [/\s+/, $.line_comment, $.block_comment],

  word: ($) => $.identifier,

  rules: {
    source_file: ($) => repeat(field("stmt", $._statement)),

    _statement: ($) =>
      choice($.if_statement, $.let_statement, $.assign_statement, $.expr_statement),

    let_statement: ($) =>
      seq(
        "let",
        field("name", $._name),
        "=",
        field("value", $._expr),
        optional(";"),
      ),

    if_statement: ($) =>
      seq(
        "if",
        field("condition", $._expr),
        field("then", $.block),
        optional(field("else_clause", $.else_clause)),
      ),

    else_clause: ($) => seq("else", field("body", $._else_body)),

    _else_body: ($) => choice($.if_statement, $.block),

    block: ($) => seq("{", repeat(field("stmt", $._statement)), "}"),

    assign_statement: ($) =>
      seq(field("target", $._expr), "=", field("value", $._expr), optional(";")),

    expr_statement: ($) => seq(field("expr", $._expr), optional(";")),

    _expr: ($) =>
      choice(
        $.binary_expr,
        $.unary_expr,
        $.field_expr,
        $.index_expr,
        $.call_expr,
        $.struct_literal,
        $.paren_expr,
        $.var_ref,
        $._literal,
      ),

    binary_expr: ($) => {
      const table = [
        [PREC.or, "or"],
        [PREC.and, "and"],
        [PREC.compare, choice("==", "!=", "<", ">", "<=", ">=")],
        [PREC.add, choice("+", "-")],
      ];
      return choice(
        ...table.map(([p, op]) =>
          prec.left(p, seq(field("lhs", $._expr), field("op", op), field("rhs", $._expr))),
        ),
      );
    },

    unary_expr: ($) =>
      choice(
        prec(PREC.not, seq(field("op", "not"), field("operand", $._expr))),
        prec(PREC.unary, seq(field("op", "-"), field("operand", $._expr))),
      ),

    field_expr: ($) =>
      prec(
        PREC.postfix,
        seq(field("base", $._expr), ".", field("field_name", $._name)),
      ),

    index_expr: ($) =>
      prec(
        PREC.postfix,
        seq(field("base", $._expr), "[", field("index", $._expr), "]"),
      ),

    call_expr: ($) =>
      prec.dynamic(
        1,
        prec(
          PREC.postfix,
          seq(
            field("callee", $._call_callee),
            field("args", $.arg_list),
          ),
        ),
      ),

    _call_callee: ($) => choice($.var_ref, $.field_expr, $.index_expr, $.paren_expr),

    arg_list: ($) => seq("(", sepBy(",", field("arg", $.arg)), ")"),
    arg: ($) => field("expr", $._expr),

    struct_literal: ($) =>
      seq(
        field("type_name", $.type_identifier),
        "{",
        sepBy(",", field("field", $.struct_field)),
        "}",
      ),

    struct_field: ($) =>
      seq(field("name", $._name), ":", field("value", $._expr)),

    paren_expr: ($) => seq("(", field("expr", $._expr), ")"),

    var_ref: ($) => field("name", $._name),

    _literal: ($) =>
      choice(
        $.int_literal,
        $.float_literal,
        $.string_literal,
        $.true_literal,
        $.false_literal,
        $.null_literal,
      ),

    _name: ($) => choice($.identifier, $.type_identifier),

    type_identifier: () => token(prec(1, /[A-Z][A-Za-z0-9_]*/)),
    identifier: () => token(prec(0, /[A-Za-z_][A-Za-z0-9_]*/)),

    int_literal: () => /\d+/,
    float_literal: () => /\d+\.\d+/,
    string_literal: () => choice(/"([^"\\]|\\.)*"/, /'([^'\\]|\\.)*'/),
    true_literal: () => "true",
    false_literal: () => "false",
    null_literal: () => "null",

    line_comment: () => token(seq("//", /[^\n]*/)),
    block_comment: () => token(nested("/*", "*/")),
  },
});
