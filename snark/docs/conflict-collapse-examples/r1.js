module.exports = grammar({ name: "r1", extras: ($) => [/\s/],
  conflicts: ($) => [[$.pair, $.single]],
  rules: {
    source: ($) => repeat1($._chunk),
    _chunk: ($) => choice($.pair, $.single),
    pair:   ($) => prec.dynamic(1, seq($.x, $.x)),
    single: ($) => prec.dynamic(0, $.x),
    x: ($) => "x",
  }});
