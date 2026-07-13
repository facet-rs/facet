function sepBy(sep, rule) {
  return optional(seq(rule, repeat(seq(sep, rule)), optional(sep)));
}

module.exports = grammar({
  name: "cfg",

  extras: ($) => [/\s+/],

  word: ($) => $.identifier,

  rules: {
    source_file: ($) => choice(field("expr", $.cfg), field("triple", $.triple)),

    cfg: ($) => seq("cfg", "(", field("expr", $._expr), ")"),

    _expr: ($) => choice($.any, $.all, $.not, $.key_value, $.atom),

    any: ($) => seq("any", "(", sepBy(",", field("expr", $._expr)), ")"),

    all: ($) => seq("all", "(", sepBy(",", field("expr", $._expr)), ")"),

    not: ($) => seq("not", "(", field("expr", $._expr), ")"),

    key_value: ($) =>
      seq(field("key", $.identifier), "=", field("value", $.string)),

    atom: ($) => field("name", $.identifier),

    identifier: () => /[A-Za-z_][A-Za-z0-9_]*/,

    triple: () => /[A-Za-z0-9_][A-Za-z0-9_.+-]*(-[A-Za-z0-9_][A-Za-z0-9_.+-]*)+/,

    string: () => /"([^"\\]|\\.)*"/,
  },
});
