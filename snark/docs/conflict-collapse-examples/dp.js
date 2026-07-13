module.exports = grammar({ name: "dp", extras: ($) => [/\s/],
  conflicts: ($) => [[$.x, $.y]],
  rules: {
    source: ($) => choice($.x, $.y),
    x: ($) => prec.dynamic(2, $.ident),
    y: ($) => prec.dynamic(1, $.ident),
    ident: ($) => /[a-z]+/,
  }});
