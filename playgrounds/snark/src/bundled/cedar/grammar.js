/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

function commaSep1(rule) {
  return seq(rule, repeat(seq(',', rule)));
}

function commaSep(rule) {
  return optional(commaSep1(rule));
}

module.exports = grammar({
  name: 'cedar',

  word: $ => $.identifier,
  extras: $ => [
    /\s/,
    $.comment,
  ],

  rules: {
    policy_set: $ => repeat($.policy),
    policy: $ => seq(
      optional($.template_declaration),
      repeat($.annotation),
      $.effect,
      '(',
      $.scope,
      ')',
      repeat($.condition),
      ';',
    ),

    template_declaration: $ => seq('template', '(', commaSep($.slot), ')', '=>'),

    effect: _ => choice('permit', 'forbid'),

    annotation: $ => seq(
      '@',
      $.identifier,
      optional(seq('(', $.string, ')')),
    ),

    scope: $ => seq(
      $.principal,
      ',',
      $.action,
      ',',
      $.resource,
    ),

    principal: $ => seq('principal', optional($.scope_constraint)),
    action: $ => seq('action', optional($.scope_constraint)),
    resource: $ => seq('resource', optional($.scope_constraint)),

    scope_constraint: $ => choice(
      seq('==', choice($.entity_reference, $.slot)),
      seq('in', choice($.entity_reference, $.entity_list, $.slot)),
      seq('is', $.name, optional(seq('in', $.entity_reference))),
    ),

    condition: $ => seq(
      choice('when', 'unless'),
      '{',
      $.expression,
      '}',
    ),

    expression: $ => choice(
      $.if_expression,
      $.or_expression,
    ),

    if_expression: $ => prec.right(0,
      seq(
        'if',
        $.expression,
        'then',
        $.expression,
        'else',
        $.expression,
      ),
    ),

    or_expression: $ => prec.left(1,
      seq(
        $.and_expression,
        repeat(
          seq(
            choice('||', '|'),
            $.and_expression,
          ),
        ),
      ),
    ),

    and_expression: $ => prec.left(2,
      seq(
        $.relation_expression,
        repeat(
          seq(
            choice('&&', '&'),
            $.relation_expression,
          ),
        ),
      ),
    ),

    relation_expression: $ => choice(
      prec.left(3, seq($._add_expression, choice('==', '!=', '<', '<=', '>', '>=', '='), $._add_expression)),
      prec.left(3, seq($._add_expression, 'has', $._add_expression)),
      prec.left(3, seq($._add_expression, 'like', $.string)),
      prec.left(3, seq($._add_expression, 'is', $.name, optional(seq('in', $._add_expression)))),
      prec.left(3, seq($._add_expression, 'in', $._add_expression)),
      $._add_expression,
    ),

    _add_expression: $ => prec.left(4,
      seq(
        $._multiply_expression,
        repeat(
          seq(
            choice('+', '-'),
            $._multiply_expression,
          ),
        ),
      ),
    ),

    _multiply_expression: $ => prec.left(5,
      seq(
        $.unary_expression,
        repeat(
          seq(
            choice('*', '/', '%'),
            $.unary_expression,
          ),
        ),
      ),
    ),

    unary_expression: $ => choice(
      prec(6, seq(choice('!', '-'), $.unary_expression)),
      $.member_expression,
    ),

    member_expression: $ => prec.left(7,
      seq(
        $._primary,
        repeat($._accessor),
      ),
    ),

    _accessor: $ => choice(
      $.field_access,
      $.method_call,
      $.index_access,
    ),

    field_access: $ => seq('.', $.identifier),

    method_call: $ => seq(
      '.',
      $.identifier,
      '(',
      commaSep($.expression),
      ')',
    ),

    index_access: $ => seq('[', $.string, ']'),

    _primary: $ => choice(
      $.true,
      $.false,
      $.integer,
      $.string,
      $.slot,
      $.entity_reference,
      $.extension_call,
      $.parenthesized_expression,
      $.list_expression,
      $.record_expression,
      $.variable,
      $.identifier,
    ),

    extension_call: $ => prec(8,
      seq(
        $.name,
        '(',
        commaSep($.expression),
        ')',
      ),
    ),

    parenthesized_expression: $ => seq('(', $.expression, ')'),

    list_expression: $ => seq(
      '[',
      commaSep($.expression),
      optional(','),
      ']',
    ),

    record_expression: $ => seq(
      '{',
      commaSep($.record_entry),
      optional(','),
      '}',
    ),

    record_entry: $ => seq(
      choice($.identifier, $.string),
      ':',
      $.expression,
    ),

    entity_reference: $ => seq(
      $.identifier,
      repeat(seq('::', $.identifier)),
      '::',
      choice($.string, $.entity_record),
    ),

    entity_record: $ => seq(
      '{',
      commaSep($.ref_init),
      optional(','),
      '}',
    ),

    ref_init: $ => seq(
      $.identifier,
      ':',
      $._literal,
    ),

    _literal: $ => choice(
      $.true,
      $.false,
      $.integer,
      $.string,
    ),

    entity_list: $ => seq(
      '[',
      commaSep1($.entity_reference),
      optional(','),
      ']',
    ),

    name: $ => prec.right(seq(
      $.identifier,
      repeat(seq('::', $.identifier)),
    )),

    slot: $ => seq('?', $.identifier, optional(seq(':', $.type_reference))),
    type_reference: $ => prec.left(seq($.name, optional(seq('<', commaSep1($.type_reference), '>')))),

    variable: _ => choice('principal', 'action', 'resource', 'context'),

    true: _ => 'true',
    false: _ => 'false',

    integer: _ => token(/[0-9]+/),

    string_content: _ => token.immediate(/[^"\\\r\n]+/),
    string: $ => seq(
      '"',
      repeat(
        choice(
          $.string_content,
          $.escape_sequence,
        ),
      ),
      optional(token.immediate('"')),
    ),

    escape_sequence: _ => token.immediate(
      seq(
        '\\',
        choice(
          /[^xu]/,
          /x[0-9a-fA-F]{2}/,
          /u\{[0-9a-fA-F]+\}/,
        ),
      ),
    ),

    identifier: _ => token(/[_a-zA-Z][_a-zA-Z0-9]*/),
    comment: _ => token(seq('//', /.*/)),
  },
});
