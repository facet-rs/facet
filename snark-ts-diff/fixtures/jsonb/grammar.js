module.exports = grammar({ name: "jsonb", extras: ($) => [/\s/],
  rules: {
    document: ($) => $._value,
    _value: ($) => choice($.object, $.array, $.string, $.number, "true", "false", "null"),
    object: ($) => seq("{", optional(seq($.pair, repeat(seq(",", $.pair)))), "}"),
    pair: ($) => seq($.string, ":", $._value),
    array: ($) => seq("[", optional(seq($._value, repeat(seq(",", $._value)))), "]"),
    string: ($) => /"[^"]*"/,
    number: ($) => /-?\d+(\.\d+)?/,
  }});
