module.exports = grammar({ name: "c1b", extras: ($) => [/\s/],
  conflicts: ($) => [[$.filter, $.call], [$.filter, $.filter]],
  rules: {
    source: ($) => $._e,
    _e: ($) => choice($.filter, $.call, $.ident),
    filter: ($) => prec.dynamic(1,  seq($._e, "|", $.ident, optional($.args))),
    call:   ($) => prec.dynamic(-1, seq($._e, $.args)),
    args:   ($) => seq("(", optional($._e), ")"),
    ident:  ($) => /[a-z]+/,
  }});
