/**
 * @file NGINX Configuration grammar for tree-sitter
 * @author Jonathan Coole
 * @license GPLv3
 */

module.exports = grammar({
  name: 'nginx',
  extras: $ => [
    $.comment,
    /[\s\p{Zs}\uFEFF\u2060\u200B]/,
  ],

  rules: {
    conf: $ => repeat($._directives),
    
    comment: $ => token(prec(-1,/#.*\n/)),
    
    ////////////////////////////////////////
    // Directive Type Choice Begins
    ////////////////////////////////////////
    
    _directives: $ => choice(
      $.simple_directive,
      $.block_directive,
      $.lua_block_directive,
    ),
    
    directive: $ => /\w+/,
    
    simple_directive: $ => seq(
      field('name', $.directive),
      repeat($.param),
      ';'
    ),
    
    block_directive: $ => seq(
      field('name', $.directive),
      repeat($.param),
      $.block
    ),

    ////////////////////////////////////////
    // Directive Type Choice Ends
    ////////////////////////////////////////
    
    
    ////////////////////////////////////////
    // Bracket Type Parsing Begins
    ////////////////////////////////////////
    
    block: $ => seq(
      '{',
      repeat($._directives),
      '}'
    ),
    
    parenthese: $ => seq(
      '(',
      repeat($.param),
      ')'
    ),
    
    bracket: $ => seq(
      '[',
      repeat($.param),
      ']'
    ),
    
    ////////////////////////////////////////
    // Bracket Type Parsing Ends
    ////////////////////////////////////////
    
    param: $ => choice(
      $.parenthese,
      $.bracket,
      $.uri,
      $.number,
      $.metric,
      $.ipv4,
      $.variable,
      $.regex,
      $.modifier,
      $.string,
      $.generic
    ),
    
    ////////////////////////////////////////
    // Simple Parameter Types Begin
    ////////////////////////////////////////
    
    generic: $ => /[\w\/\-\.]*[A-Za-z][\w\/\-=,?]+/,
    
    // Metrics are denoted numeric values: 1k 4m
    metric: $ => /\d+[A-Za-z]+/,
    
    // Variables are params that begin with $
    variable: $ => choice(
      /\$\w+/,
      /\$\{\w+\}/,
    ),
    
    number: $ => /\d+/,
    
    ////////////////////////////////////////
    // Simple Parameter Types Ends
    ////////////////////////////////////////
    
    ////////////////////////////////////////
    // Quoted String Parsing Begins
    ////////////////////////////////////////
    sq_string_content: $ => /[^\']/,
    dq_string_content: $ => /[^\"]/,
    string: $ => choice(
      seq(
        '\'',
        repeat(
          choice(
            $.sq_string_content,
            $.variable,
          )
        ),
        '\'',
      ),
      seq(
        '"',
        repeat(
          choice(
            $.dq_string_content,
            $.variable,
          )
        ),
        '"',
      ),
    ),
    ////////////////////////////////////////
    // Quoted String Parsing Ends
    ////////////////////////////////////////
    
    
    ////////////////////////////////////////
    // Regex Parsing Begins
    ////////////////////////////////////////
    regex: $ => choice(
      seq(
        choice('/', '\.', $.escaped_dot),
        optional(field('pattern', $.regex_pattern)),
      ),
      $._regex_tokens,
    ),
    regex_pattern: $ => token.immediate(prec(-1,
      repeat1(choice(            // square-bracket-delimited character class
        seq('\\', /./), // escaped character
        /[^/\\\[\n\s]/    // any character besides '[', '\', '/', '\n', ' '
      ))
    )),
    _regex_tokens: $ => choice(
      $._option,
      $._colon,
      $._or,
      $._star,
      $._eol,
      $._carrot,
      $._plus,
    ),
    ////////////////////////////////////////
    // Regex Parsing Ends
    ////////////////////////////////////////
    
    
    // Parameter Tokens
    _colon: $ => token.immediate(':'),
    _or: $ => token.immediate('|'),
    _option: $ => token.immediate('?'),
    _carrot: $ => token('^'),
    _star: $ => token('*'),
    escaped_dot: $ => token(seq('\\', /\./)),
    _eol: $ => token('$'),
    _plus: $ => token('+'),
    
    // Location modifiers
    _eq: $ => token('='),
    _tild: $ => token('~'),
    _not: $ => token('!'),
    _ts_modifier: $ => token(seq('~', '*')),
    _st_modifier: $ => token(seq('*', '~')),
    
    modifier: $ => choice(
      $._not,
      $._tild,
      $._eq,
      $._st_modifier,
      $._ts_modifier,
    ),
    
    
    ////////////////////////////////////////
    // Specially recognized params begins
    ////////////////////////////////////////
    scheme: $ => /\w+:\/\//,
    ipv4: $ => /((\d{1,3}\.){3}\d{1,3})/,
    uri: $ => seq(
      field('scheme', $.scheme),
      choice(
        $.variable,
        $.ipv4,
        $.generic
      )
    ),
    ////////////////////////////////////////
    // Specially recognized params ends
    ////////////////////////////////////////
    
    
    ////////////////////////////////////////
    // Lua Block Parsing Begins
    ////////////////////////////////////////
    
    _lua_block_directives: $ => choice(
      'access_by_lua_block',
      'header_filter_by_lua_block',
      'body_filter_by_lua_block',
      'log_by_lua_block',
      'balancer_by_lua_block',
      'content_by_lua_block',
      'rewrite_by_lua_block'
    ),
    
    lua_block_directive: $ => seq(
      $._lua_block_directives,
      $.lua_block
    ),
    
    lua_block: $ => seq(
      '{',
      repeat1($.lua_code),
      '}'
    ),
    
    lua_code: $ => $._lua_code,
    _lua_code: $ => seq(
      /[^\{\}\"\']+/,
      optional(seq(
        '{',
        repeat($._lua_code),
        '}'
      )),
      // TODO: Strings cause redundant lua_code
      optional(seq(
        '"',
        /[^\"]+/,
        '"'
      )),
      optional(seq(
        "'",
        /[^\']+/,
        "'"
      ))
    ),
    ////////////////////////////////////////
    // Lua Block Parsing Ends
    ////////////////////////////////////////
    
  }
});
