/**
 * @file Thrift grammar for tree-sitter
 * @author Amaan Qureshi <amaanq12@gmail.com>
 * @author Campbell He <campbell.he@icloud.com>
 * @license MIT
 * @see {@link https://thrift.apache.org|official website}
 * @see {@link https://thrift.apache.org/docs/idl.html|official syntax spec}
 * @see {@link http://diwakergupta.github.io/thrift-missing-guide|unofficial but comprehensive guide}
 */

// deno-lint-ignore-file ban-ts-comment
/* eslint-disable arrow-parens */
/* eslint-disable camelcase */
/* eslint-disable-next-line spaced-comment */
/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

/**
* Creates a rule to match one or more of the rules separated by the separator
* and optionally adds a trailing separator (default is false).
*
* @param {Rule} rule
* @param {string} separator - The separator to use.
* @param {boolean?} trailing_separator - The trailing separator to use.
*
* @return {SeqRule}
*
*/
function list_seq(rule, separator, trailing_separator = false) {
  return trailing_separator ?
    seq(rule, repeat(seq(separator, rule)), optional(separator)) :
    seq(rule, repeat(seq(separator, rule)));
}

const primitives = [
  'binary',
  'bool',
  'byte',
  'i8',
  'i16',
  'i32',
  'i64',
  'float',
  'double',
  'string',
  'slist',
];

const namespace_languages = [
  'as3',
  'c_glib',
  'cl',
  'cocoa',
  'cpp',
  'cpp2',
  'cpp.noexist',
  'csharp',
  'd',
  'dart',
  'delphi',
  'erl',
  'go',
  'haxe',
  'java',
  'javame',
  'js',
  'kotlin',
  'lua',
  'netcore',
  'netstd',
  'nodejs',
  'nodets',
  'noexist',
  'ocaml',
  'perl',
  'php',
  'php.path',
  'py',
  'py.twisted',
  'rb',
  'rs',
  'smalltalk.prefix',
  'smalltalk.category',
  'st',
  'swift',
  'ts',
  'xml',
  'xsd',

  // The following are deprecated and should not really be used, but are added for legacy code
  'cocoa_prefix',
  'cpp_namespace',
  'csharp_namespace',
  'delphi_namespace',
  'java_package',
  'perl_package',
  'php_namespace',
  'py_module',
  'ruby_namespace',
  'smalltalk_category',
  'smalltalk_prefix',
  'xsd_namespace',
];

// Facebook/Twitter uses these
const namespace_languages_ext = [
  'android',
  'hack',
  'hack.module',
  'hs',
  'hs2',
  'java.swift',
  'java.swift.constants',
  'json',
  'py.asyncio',
  'py3',
  'rust',
  'scala',
];

// Commented invalids are ones to have been found used by fbthrift, and so have been removed
// So, this caused a huge performance regression in tree-sitter, removing this
// lowers the state count by 200 and the large state count effectively goes to 0 (1200 -> 3)
// eslint-disable-next-line no-unused-vars
const invalid = [
  'BEGIN',
  'END',
  '__CLASS__',
  '__DIR__',
  '__FILE__',
  '__FUNCTION__',
  '__LINE__',
  '__METHOD__',
  '__NAMESPACE__',
  'abstract',
  'alias',
  'and',
  'args',
  'as',
  'assert',
  'begin',
  'break',
  'case',
  'catch',
  'class',
  'clone',
  'continue',
  'declare',
  'def',
  'default',
  'del',
  'delete',
  'do',
  'dynamic',
  'elif',
  'else',
  'elseif',
  'elsif',
  'end',
  'enddeclare',
  'endfor',
  'endforeach',
  'endif',
  'endswitch',
  'endwhile',
  'ensure',
  'except',
  'exec',
  'finally',
  'float',
  'for',
  'foreach',
  'from',
  'function',
  'global',
  'goto',
  'if',
  'implements',
  'import',
  'in',
  'inline',
  'instanceof',
  'interface',
  'is',
  'lambda',
  'module',
  'native',
  'new',
  'next',
  'nil',
  'not',
  'or',
  'package',
  'pass',
  'print',
  'private',
  'protected',
  'public',
  'raise',
  'redo',
  'rescue',
  'retry',
  'register',
  'return',
  'self',
  'sizeof',
  'static',
  'super',
  'switch',
  'synchronized',
  'then',
  'this',
  'throw',
  'transient',
  'try',
  'undef',
  'unless',
  'unsigned',
  'until',
  'use',
  'var',
  'virtual',
  'volatile',
  'when',
  'while',
  'with',
  'xor',
  'yield',
];

module.exports = grammar({
  name: 'thrift',

  extras: $ => [
    $.comment,
    /\s/,
  ],

  inline: $ => [
    $._type_identifier,
    $.list_separator,
  ],

  supertypes: $ => [
    $.definition,
    $.header,
  ],

  word: $ => $.identifier,

  rules: {
    document: $ => seq(
      repeat($.header),
      repeat($.definition),
    ),

    header: $ => choice(
      $.include_statement,
      $.namespace_declaration,
      $.package_declaration,
    ),

    include_statement: $ => seq(choice('include', 'cpp_include'), $.string),

    namespace_declaration: $ => seq(
      'namespace',
      $.namespace_scope,
      choice(alias($._type_identifier, $.namespace), $.string),
      optional($.namespace_uri),
      optional(';'),
    ),

    package_declaration: $ => seq(
      repeat($.fb_annotation_definition),
      'package',
      $.string,
    ),

    namespace_scope: _ => choice('*', ...namespace_languages, ...namespace_languages_ext),

    namespace_uri: $ => seq('(', 'uri', '=', $.string, ')'),

    definition: $ => choice(
      $.const_definition,
      $.typedef_definition,
      $.enum_definition,
      $.senum_definition,
      $.struct_definition,
      $.union_definition,
      $.exception_definition,
      $.service_definition,
      $.interaction_definition,
    ),

    const_definition: $ => seq(
      repeat($.fb_annotation_definition),
      'const',
      $.type,
      $.identifier,
      '=',
      $.literal,
      optional($.annotation_definition),
      optional($.list_separator),
    ),

    typedef_definition: $ => seq(
      repeat($.fb_annotation_definition),
      'typedef',
      $.definition_type,
      optional($.annotation_definition),
      alias($.identifier, $.typedef_identifier),
      optional($.annotation_definition),
      optional($.list_separator),
    ),

    enum_definition: $ => seq(
      repeat($.fb_annotation_definition),
      'enum',
      $._type_identifier,
      '{',
      repeat(
        seq(
          repeat($.fb_annotation_definition),
          $._identifier_with_dots,
          optional(seq('=', $.number)),
          optional($.annotation_definition),
          optional($.list_separator),
        ),
      ),
      '}',
      optional($.annotation_definition),
    ),

    senum_definition: $ => seq(
      'senum',
      $._type_identifier,
      '{',
      repeat(seq($.string, optional($.list_separator))),
      '}',
    ),

    struct_definition: $ => seq(
      repeat($.fb_annotation_definition),
      'struct',
      $._type_identifier,
      optional('xsd_all'),
      '{',
      repeat($.field),
      '}',
      optional($.annotation_definition),
    ),

    union_definition: $ => seq(
      repeat($.fb_annotation_definition),
      'union',
      $._type_identifier,
      optional('xsd_all'),
      '{',
      repeat($.field),
      '}',
      optional($.annotation_definition),
    ),

    exception_definition: $ => seq(
      repeat($.fb_annotation_definition),
      repeat($.exception_modifier),
      'exception',
      $.identifier,
      '{',
      repeat($.field),
      '}',
      optional($.annotation_definition),
    ),
    exception_modifier: _ => choice('client', 'permanent', 'server', 'safe', 'stateful', 'transient'),

    service_definition: $ => seq(
      repeat($.fb_annotation_definition),
      'service',
      $._type_identifier,
      optional(seq('extends', $._type_identifier)),
      '{',
      repeat(choice(seq('performs', $._type_identifier, ';'), $.function_definition)),
      '}',
      optional($.annotation_definition),
    ),

    // Facebook-specific
    interaction_definition: $ => seq(
      'interaction',
      $._type_identifier,
      '{',
      repeat($.function_definition),
      '}',
      optional($.annotation_definition),
    ),

    field: $ => seq(
      repeat($.fb_annotation_definition),
      optional($.field_id),
      optional($.field_modifier),
      $.type,
      optional('&'), // recursive field
      optional($.annotation_definition),
      $._identifier_with_dots,
      optional(seq('=', $.literal)),
      optional('xsd_optional'),
      optional('xsd_nillable'),
      optional($.xsd_attrs),
      optional($.annotation_definition),
      optional($.list_separator),
    ),

    field_id: $ => seq($.number, ':'),
    field_modifier: _ => choice('required', 'optional'),

    xsd_attrs: $ => seq('xsd_attrs', '{', repeat($.field), '}'),

    // Functions
    function_definition: $ => seq(
      repeat($.fb_annotation_definition),
      optional($.function_modifier),
      list_seq($.type, ','),
      optional($.annotation_definition),
      $.identifier,
      $.parameters,
      optional($.throws),
      optional($.annotation_definition),
      optional($.list_separator),
    ),

    // async is deprecated
    function_modifier: _ => choice('async', 'oneway', 'readonly', 'idempotent'),

    parameters: $ => seq('(', repeat($.parameter), ')'),
    parameter: $ => seq(
      repeat($.fb_annotation_definition),
      optional($.field_id),
      optional($.field_modifier),
      $.type,
      optional($.annotation_definition),
      $.identifier,
      optional(seq('=', $.literal)),
      optional('xsd_optional'),
      optional('xsd_nillable'),
      optional($.xsd_attrs),
      optional($.annotation_definition),
      optional($.list_separator),
    ),

    // Exceptions
    throws: $ => seq('throws', $.parameters),

    type: $ => choice(
      $._type_identifier,
      $.primitive,
      $.container_type,
      'void',
    ),

    definition_type: $ => choice(
      $.primitive,
      $.container_type,
      $._type_identifier,
    ),

    primitive: _ => choice(...primitives),

    container_type: $ => choice($.list, $.map, $.set, $.sink, $.stream),
    list: $ => prec.right(seq(
      'list',
      '<',
      $.type,
      optional($.annotation_definition),
      '>',
      optional($.annotation_definition),
      optional($.string),
    )),
    map: $ => prec.right(seq(
      'map',
      optional($.string),
      '<',
      $.type,
      optional($.annotation_definition),
      ',',
      $.type,
      optional($.annotation_definition),
      '>',
      optional($.annotation_definition),
    )),
    set: $ => prec.right(seq(
      'set',
      optional($.string),
      '<',
      $.type,
      optional($.annotation_definition),
      '>',
      optional($.annotation_definition),
    )),
    sink: $ => prec.right(seq(
      'sink',
      '<',
      $.type,
      optional($.annotation_definition),
      optional($.throws),
      ',',
      $.type,
      optional($.annotation_definition),
      optional($.throws),
      '>',
    )),
    stream: $ => prec.right(seq(
      'stream',
      '<',
      $.type,
      optional($.throws),
      '>',
    )),

    annotation_definition: $ => seq(
      '(',
      optional(
        list_seq(
          seq($.annotation_identifier, optional(seq('=', $.literal))),
          ',',
          true,
        ),
      ),
      ')',
    ),

    // @scope.Field
    // @cpp.Ref{type = cpp.RefType.Unique}
    // @python.Adapter{
    //   name = "my.module.Adapter2",
    //   typeHint = "my.another.module.AdaptedType2[]",
    // }
    fb_annotation_definition: $ => seq(
      '@',
      $.annotation_identifier,
      optional(seq(
        '{',
        repeat(
          seq($.identifier, '=', $.literal, optional(','))),
        '}',
      )),
    ),

    literal: $ => prec.right(choice(
      $.number,
      $.double,
      $.boolean,
      $.string,
      $.list_literal,
      $.map_literal,
      $.struct_literal,
      $._type_identifier,
    )),

    number: _ => {
      const hex_literal = seq(
        optional(choice('-', '+')),
        /0[xX]/,
        /[\da-fA-F](_?[\da-fA-F])*/,
      );

      const binary_literal = seq(
        optional(choice('-', '+')),
        /0[bB]/,
        /[01](_?[01])*/,
      );

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
        binary_literal,
        decimal_literal,
      ));
    },

    double: _ => /[+-]?(\d+(\.\d+)?|\.\d+)([Ee][+-]?\d+)?/,

    boolean: _ => choice('true', 'false'),

    list_literal: $ => seq(
      '[',
      repeat(seq($.literal, optional($.list_separator))),
      ']',
    ),

    map_literal: $ => seq(
      '{',
      repeat(
        seq($.literal, ':', $.literal, optional($.list_separator)),
      ),
      '}',
    ),

    struct_literal: $ => seq(
      $._type_identifier,
      '{',
      repeat(seq($.identifier, '=', $.literal, optional(','))),
      '}',
    ),

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

    // Workaround to https://github.com/tree-sitter/tree-sitter/issues/1156
    // We give names to the token_ constructs containing a regexp
    // so as to obtain a node in the CST.
    //
    unescaped_double_string_fragment: _ => token.immediate(prec(1, /[^"\\]+/)),

    // same here
    unescaped_single_string_fragment: _ => token.immediate(prec(1, /[^'\\]+/)),

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
        /U[0-9a-fA-F]{8}/,
      ))),

    identifier: _ => /[A-Za-z_][A-Za-z0-9_]*/,

    // only for enum members and fields to parse as a single toke
    // n
    _identifier_with_dots: $ => alias(/[A-Za-z_][A-Za-z0-9_.]*/, $.identifier),

    _type_identifier: $ => seq(
      field('type', $.identifier),
      repeat(seq('.', $.identifier)),
    ),

    annotation_identifier: $ => $._type_identifier,

    list_separator: _ => choice(',', ';'),

    // http://stackoverflow.com/questions/13014947/regex-to-match-a-c-style-multiline-comment/36328890#36328890
    comment: _ => token(choice(
      seq('#', /(\\(.|\r?\n)|[^\\\n])*/),
      seq('//', /(\\(.|\r?\n)|[^\\\n])*/),
      seq('/*', /[^*]*\*+([^/*][^*]*\*+)*/, '/'),
    )),
  },
});
