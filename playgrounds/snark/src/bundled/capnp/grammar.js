/**
 * @file Cap'n Proto grammar for tree-sitter
 * @author Amaan Qureshi <amaanq12@gmail.com>
 * @license MIT
 * @see {@link https://capnproto.org|official website}
 * @see {@link https://capnproto.org/language.html|official syntax spec}
 */

/* eslint-disable arrow-parens */
/* eslint-disable camelcase */
/* eslint-disable-next-line spaced-comment */
/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

/**
 * Creates a rule to match one or more of the rules separated by the separator
 * and optionally adds a trailing separator (default is false).
 *
 * @param {RegExp|Rule|String} rule
 * @param {RegExp|Rule|String} separator - The separator to use.
 * @param {boolean?} trailing_separator - The trailing separator to use.
 *
 * @return {SeqRule}
 *
 */
const list_seq = (rule, separator, trailing_separator = false) =>
  trailing_separator ?
    seq(rule, repeat(seq(separator, rule)), optional(separator)) :
    seq(rule, repeat(seq(separator, rule)));

/**
 * Creates a rule to match one or more of the rules separated by a comma
 *
 * @param {RegExp|Rule|String} rule
 *
 * @return {SeqRule}
 */
const comma_sep1 = (rule) => list_seq(rule, ',');

const primitives = [
  'AnyPointer',
  'Bool',
  'Int8',
  'Int16',
  'Int32',
  'Int64',
  'UInt8',
  'UInt16',
  'UInt32',
  'UInt64',
  'Float32',
  'Float64',
  'Text',
  'Data',
  'Void',
];

const builtin_types = [
  'annotation',
  'enum',
  'group',
  'interface',
  'struct',
  'union',
];

const annotation_targets = [
  '*',
  'const',
  'enumerant',
  'field',
  'file',
  'method',
  'param',
  ...builtin_types,
];

module.exports = grammar({
  name: 'capnp',

  extras: $ => [
    $.comment,
    /[\s\f\uFEFF\u2060\u200B]|\\\r?\n/,
  ],

  inline: $ => [
    $._single_named_return_type,

    $._annotation_identifier,
    $._const_identifier,
    $._enum_identifier,
    $._enum_member,
    $._field_identifier,
    $._generic_identifier,
    $._param_identifier,
    $._return_identifier,
    $._type_definition,
    $._type_identifier,
    $._extend_type,

    $._property,

    $._namespace,
    $._import_path,

    $._annotation_call,
  ],

  supertypes: $ => [
    $.definition,
    $.statement,
  ],

  word: $ => $.identifier,

  rules: {
    message: $ => repeat($.statement),

    unique_id: _ => token(seq('@', /0[xX][\da-fA-F](_?[\da-fA-F])*/)),

    statement: $ => choice(
      $.unique_id_statement,
      $.using_directive,
      $.import,
      $.annotation,
      $.top_level_annotation,
      $.definition,
    ),

    unique_id_statement: $ => seq($.unique_id, ';'),

    using_directive: $ => seq(
      'using',
      choice($._type_identifier, $.replace_using, $.import_using),
      ';',
    ),
    // using T = Foo.Bar;
    replace_using: $ => seq(
      $._type_definition,
      '=',
      seq($._type_identifier, optional($.generics)),
    ),
    import_using: $ => choice(
      // using import "bar.capnp".Baz;
      seq(
        'import',
        $._import_path,
        optional(repeat(seq('.', $._type_identifier))),
      ),
      // using Bar = import "bar.capnp";
      seq(
        $._type_identifier,
        '=',
        'import',
        $._import_path,
      ),
    ),

    // $import "/capnp/c++.capnp".namespace("capnp");
    import: $ => seq(
      '$import',
      choice(
        seq('(', $._import_path, ')'),
        $._import_path,
      ),
      optional(seq('.', 'namespace', '(', $._namespace, ')')),
      ';',
    ),

    _namespace: $ => alias($.string, $.namespace),

    top_level_annotation: $ => seq('$', $.top_level_annotation_body, ';'),

    // $Cxx.namespace("capnp::compiler");
    // $corge(string = "hello", number = 123);
    top_level_annotation_body: $ => seq(
      alias(token.immediate(/[A-Za-z_][A-Za-z0-9._]*/), $._type_identifier),
      optional(
        seq(
          '(',
          choice(
            $._namespace,
            $.number,
            $.float,
            $.boolean,
            $.concatenated_string,
            $.block_text,
            $.struct_shorthand,
            $._internal_const_identifier,
            $.data,
            $.const_list,
            $.void,
            $.embedded_file,
            optional(list_seq(seq($._annotation_identifier, '=', $.const_value), ',')),
          ),
          ')',
        ),
      ),
    ),

    // annotation myAnnotation(struct) :Int32 $baz(10);
    annotation: $ => seq(
      'annotation',
      $._annotation_definition_identifier,
      optional($.unique_id),
      $.annotation_targets,
      optional(seq(':', $.field_type)),
      repeat($._annotation_call),
      ';',
    ),
    annotation_targets: $ => seq('(', list_seq($.annotation_target, ','), ')'),

    annotation_target: _ => choice(...annotation_targets),

    // $Cxx.allowCancellation;
    // $TestGenerics(Text, Data).ann("foo")
    _annotation_call: $ => seq(
      '$',
      seq(
        $._annotation_identifier,
        optional($.generics),
        optional(seq('(', $._param_identifier, '=', $.const_value, ')')),
        repeat(seq('.', alias($.identifier, $.attribute))),
        optional(choice(
          seq('(', optional($.annotation_array), ')'),
          $.annotation_literal,
        )),
      ),
    ),

    annotation_literal: $ => seq('(', $.const_value, ')'),

    annotation_array: $ => choice(
      seq('[', comma_sep1($.annotation_array), ']'),
      seq('(', comma_sep1($.annotation_array), ')'),
      $._annotation_array_def,
    ),

    _annotation_array_def: $ => prec.right(1,
      choice(
        list_seq(seq($._property, '=', $.const_value), ','),
        list_seq($.const_value, ',', true),
      ),
    ),

    definition: $ => choice(
      $.struct,
      $.enum,
      $.const,
      $.interface,
    ),

    struct: $ => seq(
      'struct',
      $._type_identifier,
      optional($.generics),
      optional($.unique_id),
      repeat($._annotation_call),
      '{',
      repeat(choice($.field, $.using_directive)),
      '}',
    ),
    nested_struct: $ => $.struct,

    enum: $ => seq(
      'enum',
      $._enum_identifier,
      optional($.unique_id),
      repeat($._annotation_call),
      '{',
      repeat($.enum_field),
      '}',
    ),
    nested_enum: $ => $.enum,

    enum_field: $ => seq(
      $._enum_member,
      $.field_version,
      repeat($._annotation_call),
      ';',
    ),

    group: $ => seq(
      $._type_identifier,
      ':',
      'group',
      repeat($._annotation_call),
      '{',
      repeat($.field),
      '}',
    ),

    // for struct/group
    field: $ => choice(
      // Base type
      seq(
        $._field_identifier,
        $.field_version,
        ':',
        choice(
          seq(
            $.field_type,
            optional(seq('=', $.const_value)),
            repeat($._annotation_call),
          ),
          seq('import', $._import_path, repeat(seq('.', $._type_identifier))),
        ),
        ';',
      ),
      $.annotation,
      $.nested_struct,
      $.nested_enum,
      $.const,
      $.group,
      $.union,
      $.interface,
    ),

    union: $ => choice($._named_union, $._unnamed_union),
    nested_union: $ => $.union,
    _unnamed_union: $ => seq('union', '{', repeat($.union_field), '}'),
    _named_union: $ => seq(
      $._type_identifier,
      optional($.field_version),
      ':',
      'union',
      repeat($._annotation_call),
      '{',
      repeat($.union_field),
      '}',
    ),

    union_field: $ => choice(
      // why does this work...when we have a choice with 'union'...
      // only difference with a field is it cannot have a default value..right?
      seq(
        choice(alias('union', $.field_identifier), $._field_identifier),
        $.field_version,
        ':',
        $.field_type,
        repeat($._annotation_call),
        optional(seq('=', $.const_value)),
        ';',
      ),
      $.group,
      $.nested_union,
    ),

    interface: $ => seq(
      'interface',
      $._type_identifier,
      optional($.unique_id),
      optional($.generics),
      optional(seq('extends', '(', $._extend_type, optional($.generics), ')')),
      repeat($._annotation_call),
      '{',
      repeat(choice($.method, $.interface, $.struct, $.enum)),
      '}',
    ),

    method: $ => seq(
      $._method_identifier,
      $.field_version,
      optional($.implicit_generics),
      choice(
        // method @0 (...)
        $.method_parameters,
        // method @0 Foo
        seq($._type_identifier, optional($.generics)),
      ),
      // (...) -> (...);
      // (...) -> ();
      // (...);
      optional(seq('->', $.return_type)),
      repeat($._annotation_call),
      ';',
    ),
    method_parameters: $ => seq('(', optional($.parameters), ')'),

    parameters: $ => list_seq($.parameter, ','),

    parameter: $ => seq(
      $._param_identifier,
      ':',
      $.field_type,
      repeat($._annotation_call),
      optional(seq('=', $.const_value)),
    ),

    return_type: $ => choice(
      $.named_return_types,
      seq($.unnamed_return_type, optional($.generics)),
    ),
    named_return_types: $ => seq(
      '(',
      optional($.named_return_type),
      ')',
    ),

    unnamed_return_type: $ => $._type_identifier,

    _single_named_return_type: $ => seq(
      $._return_identifier,
      ':',
      seq($._type_identifier, optional($.generics)), // type
      optional(seq('=', $.const_value)),
    ),

    named_return_type: $ => list_seq($._single_named_return_type, ','),

    field_type: $ => choice($.primitive_type, $.list_type, $.custom_type),

    primitive_type: _ => choice(...primitives),

    list_type: $ =>
      seq(
        'List',
        '(',
        // the types have an optional : due to the field_type here, list inner types don't have a :
        $.field_type,
        ')',
      ),

    custom_type: $ => seq(
      $._type_identifier,
      optional($.generics),
      repeat(seq('.', $._type_identifier, optional($.generics))),
    ),

    const: $ => seq(
      'const',
      $._const_identifier,
      optional($.unique_id),
      ':',
      $.field_type,
      '=',
      $.const_value,
      repeat($._annotation_call),
      ';',
    ),

    const_value: $ => choice(
      $.number,
      $.float,
      $.boolean,
      $.string,
      $.concatenated_string,
      $.block_text,
      $.struct_shorthand,
      $._internal_const_identifier,
      $.data,
      $.const_list,
      $.void,
      $.embedded_file,
    ),
    _same_scope_const_value: $ => seq('.', alias($.const_value, $.local_const)),

    number: _ => {
      const hex_literal = seq(
        optional('-'),
        choice('0x', '0X'),
        /[\da-fA-F](_?[\da-fA-F])*/,
      );

      // inf, -inf, nan
      const special_literal = choice(seq(optional('-'), 'inf'), 'nan');

      const decimal_digits = /\d(_?\d)*/;
      const signed_integer = seq(optional(choice('-', '+')), decimal_digits);

      const decimal_integer_literal = choice(
        '0',
        seq(optional('0'), /[1-9]/, optional(seq(optional('_'), decimal_digits))),
      );

      const decimal_literal = choice(
        seq(optional(choice('-', '+')), decimal_integer_literal),
        decimal_digits,
        signed_integer,
      );

      return token(choice(
        hex_literal,
        decimal_literal,
        special_literal,
      ));
    },

    field_version: $ => choice($._normal_version, $._inline_version),

    // normal is just @ followed by a number
    _normal_version: _ => token(seq('@', /\d+/)),

    // inline is @ followed by a number then a !
    _inline_version: $ => alias(token(seq('@', /\d+/, '!')), $.inline_field),

    float: _ => /[+-]?(\d+(\.\d+)?|\.\d+)([Ee][+-]?\d+)?/,

    boolean: _ => choice('true', 'false'),

    // 0x"62 61 72"; # "bar"
    // data can be spaced or not?
    data: $ => seq(alias('0x', $.data_hex), alias(/"([0-9A-Fa-f]{2} ?)*"/, $.data_string)),

    const_list: $ => seq('[', optional(list_seq($.const_value, ',', true)), ']'),

    void: _ => 'void',

    struct_shorthand: $ =>
      seq(
        '(',
        repeat(
          prec.left(seq(
            $._property,
            '=',
            choice($._same_scope_const_value, $.const_value),
            optional(','),
          )),
        ),
        ')',
      ),
    _internal_const_identifier: $ => choice(
      seq(
      // Foo.Bar.Etc...
        repeat1(seq(alias($._identifier_no_period, $.type_identifier), optional('.'))),
        // ... Baz
        alias($._identifier_no_period, $.const_identifier),
      ),
      alias($._identifier_no_period, $.const_identifier),
    ),

    embedded_file: $ => seq('embed', $.string),

    generics: $ => seq('(', $.generic_parameters, ')'),
    implicit_generics: $ => seq('[', alias($.generic_parameters, $.implicit_generic_parameters), ']'),
    generic_parameters: $ => comma_sep1($._generic_identifier),

    // https://github.com/tree-sitter/tree-sitter-javascript/blob/master/grammar.js#L900-L945
    // Here we tolerate unescaped newlines in double-quoted and
    // single-quoted string literals.
    //
    string: $ => choice(
      seq(
        '"',
        repeat(choice(
          alias($.unescaped_double_string_fragment, $.string_fragment),
          $._escape_sequence,
        )),
        '"',
      ),
      seq(
        '\'',
        repeat(choice(
          alias($.unescaped_single_string_fragment, $.string_fragment),
          $._escape_sequence,
        )),
        '\'',
      ),
    ),

    // multiple string literals only separated by whitespace - a concatenated string
    concatenated_string: $ => seq($.string, repeat1($.string)),

    // block texts start with a ` only, and keep repeating until a newline
    block_text: $ => repeat1(seq(
      '`',
      repeat(choice(
        alias($.unescaped_block_string_fragment, $.string_fragment),
        $._escape_sequence,
      )),
    )),

    // Workaround to https://github.com/tree-sitter/tree-sitter/issues/1156
    // We give names to the token_ constructs containing a regexp
    // so as to obtain a node in the CST.
    unescaped_double_string_fragment: _ => token.immediate(prec(1, /[^"\\]+/)),

    // same here
    unescaped_single_string_fragment: _ => token.immediate(prec(1, /[^'\\]+/)),

    // same here (x2)
    unescaped_block_string_fragment: _ => token.immediate(prec(1, /[^`\\;]+/)),

    _escape_sequence: $ =>
      choice(
        prec(2, token.immediate(seq('\\', /[^abfnrtvxu'\"\\\?]/))),
        prec(1, $.escape_sequence),
      ),

    escape_sequence: _ => token.immediate(seq(
      '\\',
      choice(
        /[^xu0-7]/,
        /[0-7]{1,3}/,
        /x[0-9a-fA-F]{2}/,
        /u[0-9a-fA-F]{4}/,
        /u\{[0-9a-fA-F]+\}/,
      ),
    )),

    _import_path: $ => alias($.string, $.import_path),

    identifier: _ => token(/[A-Za-z_][A-Za-z0-9._]*/),
    _identifier_no_period: _ => token(/[A-Za-z_][A-Za-z0-9_]*/),

    _annotation_definition_identifier: $ => alias($.identifier, $.annotation_definition_identifier),
    _annotation_identifier: $ => alias($.identifier, $.annotation_identifier),
    _const_identifier: $ => alias($.identifier, $.const_identifier),
    _enum_identifier: $ => alias($.identifier, $.enum_identifier),
    _enum_member: $ => alias($.identifier, $.enum_member),
    _field_identifier: $ => alias($.identifier, $.field_identifier),
    _generic_identifier: $ => alias($.field_type, $.generic_identifier),
    _method_identifier: $ => alias($.identifier, $.method_identifier),
    _param_identifier: $ => alias($.identifier, $.param_identifier),
    _return_identifier: $ => alias($.identifier, $.return_identifier),
    _type_definition: $ => alias($.identifier, $.type_definition),
    _type_identifier: $ => alias($.identifier, $.type_identifier),
    _extend_type: $ => alias($.identifier, $.extend_type),

    _property: $ => alias($.identifier, $.property),

    comment: _ => token(repeat1(seq('#', /.*/))),
  },
});
