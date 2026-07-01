module.exports = grammar({ name: "c1", extras: ($) => [/\s/],
  conflicts: ($) => [[$.filter, $.call]],
  rules: {
    source: ($) => $._e,
    _e: ($) => choice($.filter, $.call, $.ident),
    filter: ($) => prec.dynamic(1,  prec.left(2, seq($._e, "|", $.ident, optional($.args)))),
    call:   ($) => prec.dynamic(-1, prec.left(2, seq($._e, $.args))),
    args:   ($) => seq("(", optional($._e), ")"),
    ident:  ($) => /[a-z]+/,
  }});
