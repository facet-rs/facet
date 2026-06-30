// Scanner-free HTML subset for the Snark playground.
//
// Raw text and comments use declarative lexical primitives. Implicit closing
// is table-driven via auto_close; no external scanner is involved.

module.exports = grammar({
  name: "html",

  // Text nodes own whitespace between tags, and tag-internal whitespace is
  // explicit, so there are no global extras.
  extras: ($) => [],

  rules: {
    document: ($) => repeat($._node),

    _node: ($) => choice($.doctype, $.element, $.comment, $.text),

    element: ($) => choice($.normal_element, $.self_closing_element, $.void_element),

    doctype: ($) =>
      seq(
        "<!",
        $.doctype_name,
        optional(seq($._ws, $.doctype_value)),
        ">",
      ),

    doctype_name: ($) => choice("DOCTYPE", "doctype"),
    doctype_value: ($) => token(until(">")),

    normal_element: ($) =>
      seq(
        $.start_tag,
        repeat($._node),
        choice($.end_tag, $.implicit_end_tag),
      ),

    start_tag: ($) =>
      seq(
        "<",
        field("name", $.tag_name),
        repeat($.attribute),
        optional($._ws),
        ">",
      ),

    end_tag: ($) =>
      seq(
        "</",
        field("name", $.tag_name),
        optional($._ws),
        ">",
      ),

    self_closing_element: ($) =>
      seq(
        "<",
        field("name", $.tag_name),
        repeat($.attribute),
        optional($._ws),
        "/>",
      ),

    void_element: ($) =>
      seq(
        "<",
        field("name", $.void_tag_name),
        repeat($.attribute),
        optional($._ws),
        ">",
      ),

    implicit_end_tag: ($) =>
      auto_close({
        tag: "implicit_end_tag",
        open_node: "start_tag",
        close_node: "end_tag",
        tag_name_node: "tag_name",
        start_prefix: "<",
        end_prefix: "</",
        rules: [
          {
            tag: "p",
            closed_by_tags: [
              "address", "article", "aside", "blockquote", "div", "dl",
              "fieldset", "footer", "form", "h1", "h2", "h3", "h4", "h5",
              "h6", "header", "hr", "main", "nav", "ol", "p", "pre",
              "section", "table", "ul",
            ],
          },
          { tag: "li", closed_by_tags: ["li"] },
          { tag: "dt", closed_by_tags: ["dt", "dd"] },
          { tag: "dd", closed_by_tags: ["dt", "dd"] },
          { tag: "rt", closed_by_tags: ["rt", "rp"] },
          { tag: "rp", closed_by_tags: ["rt", "rp"] },
          { tag: "option", closed_by_tags: ["option", "optgroup"] },
          { tag: "optgroup", closed_by_tags: ["optgroup"] },
          { tag: "thead", closed_by_tags: ["tbody", "tfoot"] },
          { tag: "tbody", closed_by_tags: ["tbody", "tfoot"] },
          { tag: "tfoot", closed_by_tags: ["tbody"] },
          { tag: "tr", closed_by_tags: ["tr"] },
          { tag: "td", closed_by_tags: ["td", "th"] },
          { tag: "th", closed_by_tags: ["td", "th"] },
        ],
      }),

    attribute: ($) =>
      seq(
        $._ws,
        $.attribute_name,
        optional(
          seq(
            optional($._ws),
            "=",
            optional($._ws),
            choice($.quoted_attribute_value, $.attribute_value),
          ),
        ),
      ),

    quoted_attribute_value: ($) =>
      choice(
        seq('"', optional($.double_quoted_attribute_value), '"'),
        seq("'", optional($.single_quoted_attribute_value), "'"),
      ),

    double_quoted_attribute_value: ($) => token(until('"')),
    single_quoted_attribute_value: ($) => token(until("'")),

    attribute_value: ($) => /[^\s"'=<>`]+/,

    tag_name: ($) => /[A-Za-z][A-Za-z0-9:-]*/,
    void_tag_name: ($) =>
      choice(
        "area",
        "base",
        "br",
        "col",
        "embed",
        "hr",
        "input",
        "link",
        "meta",
        "source",
        "track",
        "wbr",
      ),
    attribute_name: ($) => /[A-Za-z_:][A-Za-z0-9_:.-]*/,

    comment: ($) => token(seq("<!--", until("-->"), "-->")),

    text: ($) => token(prec(-1, until("<"))),

    _ws: ($) => /[\t\n\f\r ]+/,
  },
});
