// Minimal markdown-style code-fence host grammar. Prose is opaque text; each
//   ```<language>
//   …body…
//   ```
// block exposes a `language` label and a raw `body`, which the bundle's
// queries/injections.scm injects into the named embedded language. This is the
// canonical language-injection demo: one document, many grammars.
//
// Uses Snark's declarative `until` primitive for the raw runs (no scanner).

module.exports = grammar({
  name: "fences",

  // Whitespace in prose is significant, so no extras.
  extras: ($) => [],

  rules: {
    document: ($) => repeat($._block),

    _block: ($) => choice($.fence, $.text),

    // Prose between fences: everything up to the next ``` (or EOF).
    text: ($) => token(prec(-1, until("```"))),

    fence: ($) =>
      seq(
        "```",
        field("language", $.language),
        "\n",
        field("body", $.body),
        "```",
      ),

    language: ($) => /[A-Za-z0-9_+-]+/,

    // Fenced content: everything up to the closing ```.
    body: ($) => token(until("```")),
  },
});
