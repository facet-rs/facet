/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

function commaSep1(rule) {
  return seq(rule, repeat(seq(',', rule)));
}

function commaSep(rule) {
  return optional(commaSep1(rule));
}

module.exports = grammar({
  name: 'cedarschema',

  word: $ => $.identifier,
  extras: $ => [
    /\s/,
    $.comment,
  ],

  rules: {
    schema: $ => repeat(
      choice(
        $.namespace,
        $._declaration,
      ),
    ),

    namespace: $ => seq(
      repeat($.annotation),
      'namespace',
      $.name,
      '{',
      repeat(choice($.namespace, $._declaration)),
      '}',
    ),

    _declaration: $ => choice(
      $.entity_declaration,
      $.action_declaration,
      $.common_type_declaration,
    ),

    entity_declaration: $ => seq(
      repeat($.annotation),
      'entity',
      $.identifier_list,
      choice(
        seq(
          optional($.entity_parents),
          optional(seq(optional('='), $.record_type)),
          optional($.entity_tags),
          ';',
        ),
        $.enum_type,
      ),
    ),

    action_declaration: $ => seq(
      repeat($.annotation),
      'action',
      $.action_name_list,
      optional($.action_parents),
      optional($.applies_to),
      optional($.action_attributes),
      ';',
    ),

    action_attributes: $ => seq(
      'attributes',
      '{',
      commaSep($.attribute_entry),
      optional(','),
      '}',
    ),

    attribute_entry: $ => seq(
      choice($.identifier, $.string),
      ':',
      $._literal,
    ),

    _literal: $ => choice(
      $.true,
      $.false,
      $.integer,
      $.string,
    ),

    true: _ => 'true',
    false: _ => 'false',
    integer: _ => token(/[0-9]+/),

    common_type_declaration: $ => seq(
      repeat($.annotation),
      'type',
      $.identifier,
      '=',
      $._type_expression,
      ';',
    ),

    entity_parents: $ => seq(
      'in',
      $.type_list,
    ),

    entity_tags: $ => seq(
      'tags',
      $._type_expression,
    ),

    enum_type: $ => seq(
      'enum',
      '[',
      commaSep1($.string),
      optional(','),
      ']',
      ';',
    ),

    action_parents: $ => seq(
      'in',
      $.qualified_name_list,
    ),

    _applies_to_member: $ => choice(
      $.principal_types,
      $.resource_types,
      $.context_type,
    ),

    applies_to: $ => seq(
      'appliesTo',
      '{',
      commaSep($._applies_to_member),
      optional(','),
      '}',
    ),

    principal_types: $ => seq(
      'principal',
      ':',
      $.type_list,
    ),

    resource_types: $ => seq(
      'resource',
      ':',
      $.type_list,
    ),

    context_type: $ => seq(
      'context',
      ':',
      choice(
        $.record_type,
        $.name,
      ),
    ),

    _type_expression: $ => choice(
      $.primitive_type,
      $.set_type,
      $.record_type,
      $.name,
    ),

    primitive_type: _ => choice(
      'Bool',
      'Long',
      'String',
    ),

    set_type: $ => seq(
      'Set',
      '<',
      $._type_expression,
      '>',
    ),

    record_type: $ => seq(
      '{',
      commaSep($.attribute_declaration),
      optional(','),
      '}',
    ),

    attribute_declaration: $ => seq(
      repeat($.annotation),
      $._attribute_name,
      optional('?'),
      ':',
      $._type_expression,
    ),

    annotation: $ => seq(
      '@',
      $.identifier,
      optional(seq('(', $.string, ')')),
    ),

    type_list: $ => choice(
      seq('[', commaSep($.name), optional(','), ']'),
      $.name,
    ),

    qualified_name: $ => choice(
      seq($.identifier, repeat(seq('::', $.identifier)), '::', $.string),
      $._attribute_name,
    ),

    qualified_name_list: $ => choice(
      seq('[', commaSep1($.qualified_name), optional(','), ']'),
      $.qualified_name,
    ),

    name: $ => prec.right(seq(
      $.identifier,
      repeat(seq('::', $.identifier)),
    )),

    identifier_list: $ => commaSep1($.identifier),
    action_name_list: $ => commaSep1($._attribute_name),

    _attribute_name: $ => choice(
      $.identifier,
      $.string,
    ),

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
