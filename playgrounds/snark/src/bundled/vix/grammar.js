// Snark grammar for vix v0 — the build language.
//
// Surface per docs/design/sketches/lua.vix.md (vixen repo): Rust-flavored, minimal
// innovation points. Distinctive constructs:
//   - path literals  p"lua-5.4.8/src"   (strings NEVER coerce to paths)
//   - flag atoms     -O2 -DLUA_USE_LINUX (typed command-vocabulary atoms)
//   - command blocks cc! { -c {src} -o {out} }  — token soup + {expr} splices;
//     per-command grammars refine these via injection later
//   - `/` as the Tree/Path join operator (mul-tier precedence)
//   - kwargs with defaults at call sites: fetch(url: "…", sha256: "…")
//
// v0 scope: exactly enough to parse the lua sketch cleanly. Grown by critique.

const PREC = {
  or: 1,
  and: 2,
  compare: 3,
  add: 4,
  mul: 5, // `/` join lives here
  unary: 6,
  postfix: 7,
};

function sepBy(sep, rule) {
  return optional(seq(rule, repeat(seq(sep, rule)), optional(sep)));
}

module.exports = grammar({
  name: "vix",

  extras: ($) => [/\s+/, $.line_comment, $.doc_comment],

  word: ($) => $.identifier,

  rules: {
    source_file: ($) => repeat($._item),

    // ---- items ----------------------------------------------------------
    _item: ($) => choice($.use_item, $.fn_item),

    use_item: ($) => seq("use", $.use_tree, ";"),
    use_tree: ($) =>
      seq(
        $.identifier,
        repeat(seq("::", $.identifier)),
        optional(seq("::", "{", sepBy(",", $.identifier), "}")),
      ),

    fn_item: ($) =>
      seq(
        optional("pub"),
        "fn",
        field("name", $.identifier),
        field("params", $.param_list),
        optional(seq("->", field("return_type", $._type))),
        field("body", $.block),
      ),

    param_list: ($) => seq("(", sepBy(",", $.param), ")"),
    param: ($) => seq(field("name", $.identifier), ":", field("type", $._type)),

    // ---- types ----------------------------------------------------------
    _type: ($) => choice($.array_type, $.type_path),
    array_type: ($) => seq("[", $._type, "]"),
    type_path: ($) => seq($.identifier, repeat(seq("::", $.identifier))),

    // ---- statements / blocks ---------------------------------------------
    block: ($) => seq("{", repeat($._statement), optional($._expr), "}"),

    _statement: ($) => choice($.let_statement, $.expr_statement),

    let_statement: ($) =>
      seq(
        "let",
        field("name", $.identifier),
        optional(seq(":", field("type", $._type))),
        "=",
        field("value", $._expr),
        ";",
      ),

    expr_statement: ($) => seq($._expr, ";"),

    // ---- expressions ------------------------------------------------------
    _expr: ($) =>
      choice(
        $.binary,
        $.unary,
        $.call,
        $.method_call,
        $.field_access,
        $.match_expr,
        $.closure,
        $.command_block,
        $.array,
        $.paren,
        $.scoped_identifier,
        $.identifier,
        $.string,
        $.path_literal,
        $.number,
        $.boolean,
      ),

    binary: ($) => {
      const table = [
        [PREC.or, "||"],
        [PREC.and, "&&"],
        [PREC.compare, choice("==", "!=", "<", "<=", ">", ">=")],
        [PREC.add, choice("+", "-")],
        [PREC.mul, choice("*", "/", "%")],
      ];
      return choice(
        ...table.map(([p, op]) =>
          prec.left(p, seq(field("left", $._expr), field("op", op), field("right", $._expr))),
        ),
      );
    },

    unary: ($) => prec(PREC.unary, seq(choice("-", "!"), $._expr)),

    call: ($) =>
      prec(
        PREC.postfix,
        seq(field("callee", choice($.identifier, $.scoped_identifier)), field("args", $.arg_list)),
      ),

    method_call: ($) =>
      prec(
        PREC.postfix,
        seq(field("receiver", $._expr), ".", field("name", $.identifier), field("args", $.arg_list)),
      ),

    field_access: ($) =>
      prec(PREC.postfix, seq(field("receiver", $._expr), ".", field("name", $.identifier))),

    arg_list: ($) => seq("(", sepBy(",", $._arg), ")"),
    _arg: ($) => choice($.kwarg, $._expr),
    kwarg: ($) => seq(field("name", $.identifier), ":", field("value", $._expr)),

    scoped_identifier: ($) => seq($.identifier, repeat1(seq("::", $.identifier))),

    closure: ($) =>
      seq("|", sepBy(",", $.identifier), "|", field("body", $._expr)),

    match_expr: ($) =>
      seq("match", field("scrutinee", $._expr), "{", sepBy(",", $.match_arm), "}"),
    match_arm: ($) => seq(field("pattern", $._pattern), "=>", field("value", $._expr)),
    _pattern: ($) => choice($.wildcard_pattern, $.identifier, $.string, $.number),
    wildcard_pattern: () => "_",

    // Flag atoms are legal ONLY as array elements (feeding [Flag] values) — never general
    // expressions. Per-state lexing then makes `a - b` vs `[-DFOO]` unambiguous by
    // construction: a state either expects a flag or a binary minus, never both.
    array: ($) => seq("[", sepBy(",", choice($.flag, $._expr)), "]"),

    paren: ($) => seq("(", $._expr, ")"),

    // ---- command blocks ----------------------------------------------------
    // `cc! { -O2 -c {src / unit} -o {out} }` — the body is command-token soup with
    // `{expr}` splices back into vix. Per-command grammars later refine the soup
    // via injection; v0 only needs the boundary + splices to be structural.
    command_block: ($) =>
      seq(
        field("command", $.identifier),
        token.immediate("!"),
        "{",
        repeat(choice($.splice, $.command_token)),
        "}",
      ),
    splice: ($) => seq("{", $._expr, "}"),
    // Anything that isn't whitespace or a brace: flags, subcommands, file names.
    command_token: () => prec(-1, /[^{}\s]+/),

    // ---- leaves ------------------------------------------------------------
    identifier: () => /[A-Za-z_][A-Za-z0-9_]*/,

    string: () => /"([^"\\]|\\.)*"/,

    // Path literal: p"…" — a DISTINCT type from strings; `/` joins Tree×Path.
    path_literal: () => /p"([^"\\]|\\.)*"/,

    // Flag atom: a typed command-vocabulary token. `-O2`, `-DLUA_USE_LINUX`, `-lm`.
    // Requires a letter immediately after the dash (so `-2` stays unary-minus number),
    // and only appears where the grammar expects it (array elements).
    flag: () => token(/--?[A-Za-z][A-Za-z0-9_=+.\/-]*/),

    number: () => /\d+(\.\d+)?/,
    boolean: () => choice("true", "false"),

    line_comment: () => token(seq("//", /[^/\n][^\n]*|/)),
    doc_comment: () => token(seq("///", /[^\n]*/)),
  },
});
