/**
 * Tree-sitter grammar for the Styx configuration language.
 *
 * Styx is a structured document format using explicit braces/parens
 * with opaque scalars and a two-layer processing model.
 *
 * Key concepts:
 * - An "expr" is an optional tag plus a payload (tag annotates payload as siblings)
 * - An "entry" has a key (first expr) and optional value (remaining exprs)
 */

module.exports = grammar({
  name: "styx",

  // External scanner handles context-sensitive constructs
  externals: ($) => [
    $._heredoc_start, // <<DELIM including the delimiter
    $._heredoc_lang, // optional ,lang after delimiter
    $._heredoc_content, // lines until closing delimiter
    $._heredoc_end, // closing delimiter
    $._raw_string_start, // r#*" opening
    $._raw_string_content, // content until closing
    $._raw_string_end, // "# closing with matching count
    $._unit_at, // @ not followed by tag name char
    $._tag_start, // @tagname (@ immediately followed by tag name)
  ],

  extras: ($) => [
    /[ \t]/, // horizontal whitespace (not newlines - those are significant)
    $.line_comment,
  ],

  conflicts: ($) => [
    // Attributes vs bare scalars
    [$.attributes],
    // Object body can be newline or comma separated
    [$._newline_separated, $._comma_separated],
  ],

  rules: {
    // Document is the root - a sequence of entries (implicit root object)
    // Allow leading newlines before content
    document: ($) =>
      seq(repeat($._newline), repeat(seq(optional($.doc_comment), $.entry, repeat($._newline)))),

    // Entry: key followed by optional value
    // - key only = implicit unit value
    // - key + value = explicit value
    entry: ($) =>
      prec.right(
        choice(
          // Key with value(s)
          seq(field("key", $.expr), field("value", $.expr), repeat(field("value", $.expr))),
          // Key only (implicit unit value)
          field("key", $.expr),
        ),
      ),

    // An expr is an optional tag followed by a payload
    // Examples:
    //   @object{...}  -> tag=@object, payload=object
    //   @string       -> tag=@string, payload=none (unit)
    //   "hello"       -> tag=none, payload=scalar
    //   {a 1}         -> tag=none, payload=object
    expr: ($) =>
      choice(
        // Tagged expr: @name followed by optional payload
        prec.right(seq(field("tag", $.tag), optional(field("payload", $._payload)))),
        // Untagged expr: just a payload
        field("payload", $._payload),
        // Attributes are a special case
        $.attributes,
      ),

    // Tag: @name (just the tag itself, not including payload)
    tag: ($) => $._tag_start,

    // Payload: the actual content (without tag)
    _payload: ($) => choice($.scalar, $.sequence, $.object, $.unit),

    // Scalars: four types
    scalar: ($) => choice($.bare_scalar, $.quoted_scalar, $.raw_scalar, $.heredoc),

    // Bare scalar: unquoted text without special chars
    bare_scalar: ($) => /[^{}\(\),"=@\s\/][^{}\(\),"=@\s]*/,
    // Note: can't start with / to avoid confusion with comments
    // The second part allows / in the middle (for URLs like https://...)

    // Quoted scalar: "..." with escape sequences
    quoted_scalar: ($) => seq('"', repeat(choice($.escape_sequence, /[^"\\]+/)), '"'),

    escape_sequence: ($) =>
      token(choice(/\\[\\\"nrt0]/, /\\u[0-9A-Fa-f]{4}/, /\\u\{[0-9A-Fa-f]{1,6}\}/)),

    // Raw scalar: r#"..."# - handled by external scanner
    raw_scalar: ($) => seq($._raw_string_start, optional($._raw_string_content), $._raw_string_end),

    // Heredoc: <<DELIM[,lang]\n...\nDELIM - handled by external scanner
    heredoc: ($) =>
      seq(
        $._heredoc_start,
        optional(alias($._heredoc_lang, $.heredoc_lang)),
        optional(alias($._heredoc_content, $.heredoc_content)),
        $._heredoc_end,
      ),

    // Unit: bare @ (not followed by a tag name character)
    // Handled by external scanner
    unit: ($) => $._unit_at,

    // Sequence: (expr expr ...)
    sequence: ($) => seq("(", repeat(seq($.expr, optional($._ws))), ")"),

    // Object: { entries }
    object: ($) => seq("{", optional($._object_body), "}"),

    _object_body: ($) => choice($._newline_separated, $._comma_separated),

    _newline_separated: ($) =>
      seq(
        optional($._newline),
        optional($.doc_comment),
        $.entry,
        repeat(seq($._newline, optional($.doc_comment), $.entry)),
        optional($._newline),
      ),

    _comma_separated: ($) =>
      seq(
        $.entry,
        repeat(seq(",", $.entry)),
        optional(","), // trailing comma allowed
      ),

    // Attributes: key=value pairs that form an object expr
    attributes: ($) => repeat1($.attribute),

    attribute: ($) => seq(field("key", $.bare_scalar), "=", field("value", $._attribute_value)),

    _attribute_value: ($) => choice($.bare_scalar, $.quoted_scalar, $.sequence, $.object),

    // Comments
    // Line comment: // followed by space or non-/ char, then rest of line
    // This ensures /// is NOT matched as line_comment
    line_comment: ($) => token(seq("//", /[^\/]/, /[^\n\r]*/)),

    // Doc comment: /// lines (use prec to prefer over line_comment)
    doc_comment: ($) => repeat1(seq("///", /[^\n\r]*/, $._newline)),

    // Whitespace helpers
    _ws: ($) => /[ \t]+/,
    _newline: ($) => /\r?\n/,
  },
});
