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

  // The scrutinee/struct-literal split below is resolved BY CONSTRUCTION
  // (`_scrutinee` is `_expr` minus `struct_literal` — Rust's rule), but
  // tree-sitter's LR generator still needs the shared-prefix states declared
  // so it can GLR-split them; without this, `tree-sitter generate` refuses.
  // Snark's own executor is unaffected (it already handles the facts).
  conflicts: ($) => [
    [$._scrutinee, $.struct_literal],
    [$._expr, $.struct_literal],
  ],

  rules: {
    // Every AST-relevant child carries a field(): the typed AST (and its lowering) is
    // DERIVED from fields + cardinality (bare -> T, optional -> Option<T>, repeat -> Vec<T>)
    // by vix's build.rs. An unfielded child is invisible to the AST by construction.
    source_file: ($) => repeat(field("item", $._item)),

    // ---- items ----------------------------------------------------------
    _item: ($) => choice($.use_item, $.fn_item, $.struct_item, $.enum_item),

    use_item: ($) => seq("use", field("tree", $.use_tree), ";"),
    use_tree: ($) =>
      seq(
        field("segment", $.identifier),
        repeat(seq("::", field("segment", $.identifier))),
        optional(seq("::", "{", sepBy(",", field("leaf", $.identifier)), "}")),
      ),

    fn_item: ($) =>
      seq(
        optional(field("vis", "pub")),
        "fn",
        field("name", $.fn_name),
        optional(field("generics", $.generic_params)),
        field("params", $.param_list),
        optional(seq("->", field("return_type", $._type))),
        field("body", $.block),
      ),

    // Record struct `struct T { a: A, b: B = default }`, tuple struct
    // `struct T(A);` (newtype), unit struct `struct T;`. Field defaults mirror
    // kwargs-with-defaults at call sites.
    struct_item: ($) =>
      seq(
        optional(field("vis", "pub")),
        "struct",
        field("name", $.identifier),
        optional(field("generics", $.generic_params)),
        choice(field("fields", $.field_list), seq(field("tuple", $.tuple_fields), ";"), ";"),
      ),

    enum_item: ($) =>
      seq(
        optional(field("vis", "pub")),
        "enum",
        field("name", $.identifier),
        optional(field("generics", $.generic_params)),
        "{",
        sepBy(",", field("variant", $.variant)),
        "}",
      ),

    // Unit `Phony`, tuple `Object(Path)`, record `Archive { name: String }`.
    // Declaration order IS the total order — reordering variants is semantic.
    variant: ($) =>
      seq(
        field("name", $.identifier),
        optional(choice(field("tuple", $.tuple_fields), field("fields", $.field_list))),
      ),

    field_list: ($) => seq("{", sepBy(",", field("field", $.field_decl)), "}"),
    field_decl: ($) =>
      seq(
        field("name", $.identifier),
        ":",
        field("type", $._type),
        optional(seq("=", field("default", $._expr))),
      ),
    tuple_fields: ($) => seq("(", sepBy(",", field("type", $._type)), ")"),

    // Type parameters: no lifetimes (values, not places), and no hash/eq/ord
    // bounds — every vix value has them by construction.
    generic_params: ($) => seq("<", sepBy(",", field("param", $.identifier)), ">"),

    param_list: ($) => seq("(", sepBy(",", field("param", $.param)), ")"),
    param: ($) => seq(field("name", $.identifier), ":", field("type", $._type)),

    // ---- types ----------------------------------------------------------
    _type: ($) =>
      choice($.array_type, $.fn_type, $.tuple_type, $.generic_type, $.type_path),
    array_type: ($) => seq("[", field("elem", $._type), "]"),
    generic_type: ($) =>
      seq(field("base", $.type_path), "<", sepBy(",", field("arg", $._type)), ">"),
    tuple_type: ($) =>
      seq("(", field("elem", $._type), ",", sepBy(",", field("elem", $._type)), ")"),
    fn_type: ($) =>
      seq(
        "fn",
        "(",
        sepBy(",", field("param", $._type)),
        ")",
        optional(seq("->", field("return_type", $._type))),
      ),
    type_path: ($) =>
      seq(field("segment", $.identifier), repeat(seq("::", field("segment", $.identifier)))),

    // ---- statements / blocks ---------------------------------------------
    block: ($) =>
      seq("{", repeat(field("stmt", $._statement)), optional(field("tail", $._expr)), "}"),

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

    expr_statement: ($) => seq(field("expr", $._expr), ";"),

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
        $.struct_literal,
        $.map_literal,
        $.tuple_expr,
        $.array,
        $.paren,
        $.scoped_identifier,
        $.identifier,
        $.template_string,
        $.string,
        $.path_literal,
        $.number,
        $.boolean,
      ),

    // `match X {` — a bare struct literal as scrutinee would be ambiguous with
    // the match body's `{` (vix has no block/if exprs, so this is the ONLY
    // struct-literal ambiguity). Rust's rule, shallowly: parenthesize it.
    _scrutinee: ($) =>
      choice(
        $.binary,
        $.unary,
        $.call,
        $.method_call,
        $.field_access,
        $.match_expr,
        $.closure,
        $.map_literal,
        $.tuple_expr,
        $.array,
        $.paren,
        $.scoped_identifier,
        $.identifier,
        $.template_string,
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

    unary: ($) =>
      prec(PREC.unary, seq(field("op", choice("-", "!")), field("operand", $._expr))),

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

    // `.name` field access and `.0` tuple index share one node. tuple_index is
    // a DEDICATED integer token: the only states where it's valid are after
    // `.`, where `number` (with its fraction part) is NOT — so `t.0.1` lexes
    // as `0`, `.`, `1` by construction. Per-state lexing again: no float token
    // is ever formed and re-split (the rustc hack rust-analyzer hates).
    field_access: ($) =>
      prec(
        PREC.postfix,
        seq(field("receiver", $._expr), ".", field("name", choice($.identifier, $.tuple_index))),
      ),

    arg_list: ($) => seq("(", sepBy(",", field("arg", $._arg)), ")"),
    // `object(cc: gcc, ..)` — a trailing bare `..` makes the call PARTIAL:
    // the result is a function of the not-yet-given (non-defaulted) params.
    _arg: ($) => choice($.kwarg, $.partial, $._expr),
    partial: () => "..",
    kwarg: ($) => seq(field("name", $.identifier), ":", field("value", $._expr)),

    scoped_identifier: ($) =>
      seq(field("segment", $.identifier), repeat1(seq("::", field("segment", $.identifier)))),

    closure: ($) =>
      seq("|", sepBy(",", field("param", $.identifier)), "|", field("body", $._expr)),

    match_expr: ($) =>
      seq(
        "match",
        field("scrutinee", $._scrutinee),
        "{",
        sepBy(",", field("arm", $.match_arm)),
        "}",
      ),
    match_arm: ($) =>
      seq(
        field("pattern", $._pattern),
        optional(seq("if", field("guard", $._expr))),
        "=>",
        field("value", $._expr),
      ),

    _pattern: ($) =>
      choice(
        $.wildcard_pattern,
        $.variant_pattern,
        $.struct_pattern,
        $.tuple_pattern,
        $.scoped_identifier,
        $.identifier,
        $.string,
        $.number,
        $.boolean,
      ),
    wildcard_pattern: () => "_",
    // `Artifact::Object(p)` / `Some(x)` — payload patterns recurse.
    variant_pattern: ($) =>
      seq(
        field("path", choice($.identifier, $.scoped_identifier)),
        "(",
        sepBy(",", field("arg", $._pattern)),
        ")",
      ),
    // `Artifact::Archive { name, members: m, .. }` — shorthand binds the field name.
    struct_pattern: ($) =>
      seq(
        field("path", choice($.identifier, $.scoped_identifier)),
        "{",
        sepBy(",", choice(field("field", $.field_pattern), field("rest", $.rest_pattern))),
        "}",
      ),
    field_pattern: ($) =>
      seq(field("name", $.identifier), optional(seq(":", field("pattern", $._pattern)))),
    rest_pattern: () => "..",
    tuple_pattern: ($) =>
      seq("(", field("elem", $._pattern), ",", sepBy(",", field("elem", $._pattern)), ")"),

    // Flag atoms are legal ONLY as array elements (feeding [Flag] values) — never general
    // expressions. Per-state lexing then makes `a - b` vs `[-DFOO]` unambiguous by
    // construction: a state either expects a flag or a binary minus, never both.
    array: ($) => seq("[", sepBy(",", field("elem", choice($.flag, $._expr))), "]"),

    // `Toolchain { opt: 1, ..base }` — record construction with defaults
    // filling the gaps and `..base` functional update.
    struct_literal: ($) =>
      seq(
        field("path", choice($.identifier, $.scoped_identifier)),
        "{",
        sepBy(",", choice(field("field", $.field_init), field("spread", $.spread))),
        "}",
      ),
    field_init: ($) => seq(field("name", $.identifier), ":", field("value", $._expr)),
    // `..base` = record update; bare `..` = PARTIAL construction ("the rest
    // comes later" — same `..` as pattern rest and call partials).
    spread: ($) => seq("..", optional(field("base", $._expr))),

    // Bare braces are free in vix (no block/if exprs), so maps get the literal
    // they deserve: `{ "CC": "clang", "OPT": flag }` — keys are expressions.
    map_literal: ($) => seq("{", sepBy(",", field("entry", $.map_entry)), "}"),
    map_entry: ($) => seq(field("key", $._expr), ":", field("value", $._expr)),

    tuple_expr: ($) =>
      seq("(", field("elem", $._expr), ",", sepBy(",", field("elem", $._expr)), ")"),

    paren: ($) => seq("(", field("inner", $._expr), ")"),

    // ---- command blocks ----------------------------------------------------
    // `cc! { -O2 -c {src / unit} -o {out} }` — the body is command-token soup with
    // `{expr}` splices back into vix. Per-command grammars later refine the soup
    // via injection; v0 only needs the boundary + splices to be structural.
    command_block: ($) =>
      seq(
        field("command", $.identifier),
        token.immediate("!"),
        "{",
        repeat(field("part", choice($.splice, $.command_token))),
        "}",
      ),
    splice: ($) => seq("{", field("expr", $._expr), "}"),
    // Anything that isn't whitespace or a brace: flags, subcommands, file names.
    command_token: () => prec(-1, /[^{}\s]+/),

    // ---- leaves ------------------------------------------------------------
    identifier: () => /[A-Za-z_][A-Za-z0-9_]*/,

    // A function may be named by an operator symbol — the spaceship
    // `fn <=>(self: Version, other) -> Ordering` overloads comparison for the
    // receiver's type (and `< <= > >=` derive from it); `fn +` / `fn /` etc.
    // overload arithmetic. A single leaf token (identifier OR operator) so `name`
    // stays a uniform leaf with a text value, not a mixed-alternative field.
    fn_name: () =>
      token(
        choice(
          /[A-Za-z_][A-Za-z0-9_]*/,
          "<=>",
          "==",
          "!=",
          "<=",
          ">=",
          "<",
          ">",
          "+",
          "-",
          "*",
          "/",
          "%",
        ),
      ),

    // Loop-free config-generation templates. Holes are lowered into demand edges.
    template_string: () => /tmpl"([^"\\]|\\.)*"/,

    string: () => /"([^"\\]|\\.)*"/,

    // Path literal: p"…" — a DISTINCT type from strings; `/` joins Tree×Path.
    path_literal: () => /p"([^"\\]|\\.)*"/,

    // Flag atom: a typed command-vocabulary token. `-O2`, `-DLUA_USE_LINUX`, `-lm`.
    // Requires a letter immediately after the dash (so `-2` stays unary-minus number),
    // and only appears where the grammar expects it (array elements).
    flag: () => token(/--?[A-Za-z][A-Za-z0-9_=+.\/-]*/),

    number: () => /\d+(\.\d+)?/,
    tuple_index: () => /[0-9]+/,
    boolean: () => choice("true", "false"),

    line_comment: () => token(prec(1, seq("//", /[^\n]*/))),
    doc_comment: () => token(prec(2, seq("///", /[^\n]*/))),
  },
});
