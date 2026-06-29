module.exports = grammar({
  name: 'dot',

  extras: $ => [
    /\s|\r?\n/,
    $.comment,
    $.preproc,
  ],

  word: $ => $.identifier,

  rules: {
    source_file: $ => seq(
      optional(
        alias(ignoreCase('strict'), 'strict'),
      ),
      choice(
        alias(ignoreCase('graph'), 'graph'),
        alias(ignoreCase('digraph'), 'digraph'),
      ),
      optional(field('id', $.id)),
      field('block', $.block),
    ),

    block: $ => seq(
      '{',
      optional($.stmt_list),
      '}',
    ),

    stmt_list: $ => repeat1(
      seq(
        $._stmt,
        optional(';'),
      ),
    ),

    _stmt: $ => choice(
      $.subgraph,
      $.node_stmt,
      $.edge_stmt,
      $.attr_stmt,
      $.attribute,
    ),

    subgraph: $ => seq(
      optional(
        seq(
          alias(ignoreCase('subgraph'), 'subgraph'),
          optional(field('id', $.id)),
        ),
      ),
      field('block', $.block),
    ),

    node_stmt: $ => seq(
      $.node_id,
      optional($.attr_list),
    ),

    edge_stmt: $ => seq(
      choice(
        $.node_id,
        $.subgraph,
      ),
      repeat1(
        seq(
          $.edgeop,
          choice(
            $.node_id,
            $.subgraph,
          ),
        ),
      ),
      optional($.attr_list),
    ),

    attr_stmt: $ => seq(
      choice(
        alias(ignoreCase('graph'), 'graph'),
        alias(ignoreCase('node'), 'node'),
        alias(ignoreCase('edge'), 'edge'),
      ),
      $.attr_list,
    ),

    node_id: $ => seq(
      $.id,
      optional($.port),
    ),

    port: $ => seq(
      alias(':', $.operator),
      $.id,
      optional(
        seq(
          alias(':', $.operator),
          $.id,
        )
      ),
    ),

    attr_list: $ => repeat1(
      seq(
        '[',
        optional($._a_list),
        ']',
      ),
    ),

    _a_list: $ => repeat1(seq(
      $.attribute,
      optional(choice(';', ',')),
    )),

    attribute: $ => seq(
      field('name', $.id),
      alias('=', $.operator),
      field('value', $.id),
    ),

    id: $ => choice(
      $.identifier,
      $.number_literal,
      $.html_string,
      seq(
        $.string_literal,
        repeat(seq(
          alias('+', $.operator),
          $.string_literal,
        )),
      ),
    ),

    identifier: _ => /[_\p{XID_Start}][_\p{XID_Continue}]*/,

    number_literal: _ => /-?(\.[0-9]+|[0-9]+(\.[0-9]*)?)/,

    string_literal: _ => seq(
      '"',
      /([^"\\]+|\\.)*"/,
    ),

    html_string: $ => seq(
      '<',
      optional(alias($._html_internal, $.html_internal)),
      '>',
    ),

    _html_internal: $ => repeat1(
      choice(
        /[^<>]+/,
        seq(
          '<',
          optional($._html_internal),
          '>',
        ),
      ),
    ),

    edgeop: _ => choice('->', '--'),

    comment: _ => token(
      choice(
        seq('//', /.*/),
        seq(
            '/*',
            /[^*]*\*+([^/*][^*]*\*+)*/,
            '/',
        ),
      ),
    ),

    preproc: _ => seq('#', /.*/),
  }
});

function ignoreCase(word) {
    return new RegExp(
      word.split('')
        .map(char => `[${char}${char.toUpperCase()}]`)
        .join(''))
}
