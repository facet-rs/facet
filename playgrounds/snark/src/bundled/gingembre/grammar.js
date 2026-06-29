// Tree-sitter / snark grammar for gingembre, the Jinja-like template language.
//
// Mirrors gingembre-syntax/src/{lexer,parser}.rs. This is the "workaround" encoding
// that runs on the current snark-wasm WITHOUT the planned `until` / `nested` lexical
// primitives:
//   - raw text is `/[^{]+/` plus a standalone `{` node (a lone brace that does not open
//     a delimiter), instead of one `until('{{','{%','{#')` token;
//   - `{# #}` comments are a recursive RULE (nesting via self-reference), instead of one
//     `nested('{#','#}')` token.
// Both produce a noisier CST than gingembre's, but parse the same language. The two
// primitives later collapse them into single tokens.
//
// Precedence (loosest -> tightest), from gingembre-syntax/src/parser.rs:
//   ternary > or > and > not > comparison(== != < > <= >=, in, not in, is) >
//   add(+ - ~) > mul(* / // %) > unary(-) > power(**) > filter(|) >
//   postfix(. [] () ?) > primary

const PREC = {
  ternary: 1,
  or: 2,
  and: 3,
  not: 4,
  compare: 5,
  add: 6,
  mul: 7,
  unary: 8,
  power: 9,
  filter: 10,
  postfix: 11,
};

function sepBy(sep, rule) {
  return optional(seq(rule, repeat(seq(sep, rule)), optional(sep)));
}

// `{% extends/include/import EXPR [as name] %}`
function simpleStmt($, kw) {
  return seq(
    $._so,
    kw,
    optional(seq($._expr, optional(seq("as", $.identifier)))),
    $._sc,
  );
}

module.exports = grammar({
  name: "gingembre",

  word: ($) => $.identifier,

  // Whitespace is insignificant inside code regions; in raw text it is captured by the
  // `text` token (and any leading run becomes an extra, still covered losslessly).
  extras: ($) => [/\s/],

  // `{%` can begin either a nested statement or the end tag of the enclosing block; the
  // GLR runtime resolves the split on the following keyword.
  conflicts: ($) => [],

  rules: {
    template: ($) => repeat($._node),

    _node: ($) =>
      choice($.text, $.brace, $.comment, $.interpolation, $._statement),

    // Raw text run (stops at any `{`). A lone `{` that does not open a delimiter is a
    // separate node; `{{`/`{%`/`{#` win by longest-match so they are never eaten here.
    text: ($) => token(prec(-1, /[^{]+/)),
    brace: ($) => "{",

    // Nested comment as a rule: content is non-brace/hash runs, stray `{`/`#`, or a
    // nested comment.
    comment: ($) =>
      seq("{#", repeat(choice($.comment, /[^{#]+/, "{", "#")), "#}"),

    interpolation: ($) =>
      seq(choice("{{", "{{-"), optional($._expr), choice("}}", "-}}")),

    // ----- statements -----

    _statement: ($) =>
      choice(
        $.if_statement,
        $.for_statement,
        $.set_statement,
        $.block_statement,
        $.macro_statement,
        $.extends_statement,
        $.include_statement,
        $.import_statement,
        $.break_statement,
        $.continue_statement,
      ),

    _so: ($) => choice("{%", "{%-"),
    _sc: ($) => choice("%}", "-%}"),

    if_statement: ($) =>
      seq(
        $._so,
        "if",
        $._expr,
        $._sc,
        optional($.body),
        repeat($.elif_clause),
        optional($.else_clause),
        $._so,
        "endif",
        $._sc,
      ),

    elif_clause: ($) =>
      seq($._so, "elif", $._expr, $._sc, optional($.body)),

    else_clause: ($) => seq($._so, "else", $._sc, optional($.body)),

    for_statement: ($) =>
      seq(
        $._so,
        "for",
        $.identifier,
        repeat(seq(",", $.identifier)),
        "in",
        $._expr,
        $._sc,
        optional($.body),
        optional($.else_clause),
        $._so,
        "endfor",
        $._sc,
      ),

    set_statement: ($) =>
      choice(
        seq($._so, "set", $.identifier, "=", $._expr, $._sc),
        seq(
          $._so,
          "set",
          $.identifier,
          $._sc,
          optional($.body),
          $._so,
          "endset",
          $._sc,
        ),
      ),

    block_statement: ($) =>
      seq(
        $._so,
        "block",
        $.identifier,
        $._sc,
        optional($.body),
        $._so,
        "endblock",
        optional($.identifier),
        $._sc,
      ),

    macro_statement: ($) =>
      seq(
        $._so,
        "macro",
        $.identifier,
        $.param_list,
        $._sc,
        optional($.body),
        $._so,
        "endmacro",
        $._sc,
      ),

    param_list: ($) =>
      seq("(", sepBy(",", $.param), ")"),

    param: ($) => seq($.identifier, optional(seq("=", $._expr))),

    extends_statement: ($) => simpleStmt($, "extends"),
    include_statement: ($) => simpleStmt($, "include"),
    import_statement: ($) => simpleStmt($, "import"),

    break_statement: ($) => seq($._so, "break", $._sc),
    continue_statement: ($) => seq($._so, "continue", $._sc),

    body: ($) => repeat1($._node),

    // ----- expressions -----

    _expr: ($) =>
      choice(
        $.ternary,
        $.binary,
        $.unary,
        $.test,
        $.filter,
        $.field,
        $.index,
        $.call,
        $.optional,
        $.macro_call,
        $.paren,
        $.list,
        $.dict,
        $.literal,
        $.variable,
      ),

    ternary: ($) =>
      prec.right(
        PREC.ternary,
        seq($._expr, "if", $._expr, optional(seq("else", $._expr))),
      ),

    binary: ($) =>
      choice(
        prec.left(PREC.or, seq($._expr, "or", $._expr)),
        prec.left(PREC.and, seq($._expr, "and", $._expr)),
        prec.left(
          PREC.compare,
          seq($._expr, choice("==", "!=", "<", ">", "<=", ">="), $._expr),
        ),
        prec.left(PREC.compare, seq($._expr, "in", $._expr)),
        prec.left(PREC.compare, seq($._expr, "not", "in", $._expr)),
        prec.left(PREC.add, seq($._expr, choice("+", "-", "~"), $._expr)),
        prec.left(PREC.mul, seq($._expr, choice("*", "/", "//", "%"), $._expr)),
        prec.right(PREC.power, seq($._expr, "**", $._expr)),
      ),

    unary: ($) =>
      choice(
        prec.right(PREC.not, seq("not", $._expr)),
        prec.right(PREC.unary, seq("-", $._expr)),
      ),

    test: ($) =>
      prec.left(
        PREC.compare,
        seq(
          $._expr,
          "is",
          optional("not"),
          choice($.identifier, "none", "None"),
          optional($.arg_list),
        ),
      ),

    filter: ($) =>
      prec.left(
        PREC.filter,
        seq($._expr, "|", $.identifier, optional($.arg_list)),
      ),

    field: ($) => prec.left(PREC.postfix, seq($._expr, ".", $.identifier)),

    index: ($) =>
      prec.left(PREC.postfix, seq($._expr, "[", $._subscript, "]")),

    _subscript: ($) =>
      choice(
        $._expr,
        seq(optional($._expr), ":", optional($._expr)),
      ),

    call: ($) => prec.left(PREC.postfix, seq($._expr, $.arg_list)),

    optional: ($) => prec.left(PREC.postfix, seq($._expr, "?")),

    macro_call: ($) =>
      prec(
        PREC.postfix,
        seq($.identifier, "::", $.identifier, optional($.arg_list)),
      ),

    arg_list: ($) => seq("(", sepBy(",", choice($.kwarg, $.argument)), ")"),

    kwarg: ($) => seq($.identifier, "=", $._expr),
    argument: ($) => $._expr,

    paren: ($) => seq("(", $._expr, ")"),
    list: ($) => seq("[", sepBy(",", $._expr), "]"),
    dict: ($) => seq("{", sepBy(",", seq($._expr, ":", $._expr)), "}"),

    variable: ($) => $.identifier,

    literal: ($) =>
      choice($.number, $.string, $.boolean, $.none),

    boolean: ($) => choice("true", "True", "false", "False"),
    none: ($) => choice("none", "None"),

    identifier: ($) => /[A-Za-z_][A-Za-z0-9_]*/,
    number: ($) => /\d+(\.\d+)?/,
    string: ($) =>
      choice(/"([^"\\]|\\.)*"/, /'([^'\\]|\\.)*'/),
  },
});
