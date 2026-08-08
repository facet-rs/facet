module.exports = grammar({
  name: "markdown_pilot",

  extras: ($) => [],

  rules: {
    document: ($) => repeat(choice($.fenced_code_block, $.paragraph, $.blank_line)),

    paragraph: ($) => seq(field("content", $.paragraph_text), "\n"),

    paragraph_text: ($) => /[^`\n][^\n]*/,

    blank_line: ($) => "\n",

    fenced_code_block: ($) =>
      seq(
        "```",
        optional(field("info", $.info_string)),
        "\n",
        field("body", $.fence_body),
        "```",
        "\n",
      ),

    info_string: ($) => /[A-Za-z0-9_+-]+/,

    fence_body: ($) => token(until("```")),
  },
});
