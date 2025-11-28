/**
 * KDL language definition for highlight.js
 * Based on https://kdl.dev specification
 */

import type { HLJSApi, Language, Mode } from "highlight.js";

export default function kdl(hljs: HLJSApi): Language {
  const ESCAPES: Mode = {
    scope: "char.escape",
    variants: [
      { begin: /\\n/ },
      { begin: /\\r/ },
      { begin: /\\t/ },
      { begin: /\\"/ },
      { begin: /\\\\/ },
      { begin: /\\b/ },
      { begin: /\\f/ },
      { begin: /\\u\{[0-9a-fA-F]{1,6}\}/ },
    ],
  };

  const STRINGS: Mode = {
    scope: "string",
    variants: [
      // Raw strings: r#"..."#, r##"..."##, etc.
      { begin: /r(#)*"/, end: /"(#)*/ },
      // Regular strings
      { begin: /"/, end: /"/ },
    ],
    contains: [ESCAPES],
  };

  const COMMENTS: Mode = {
    scope: "comment",
    variants: [
      hljs.C_BLOCK_COMMENT_MODE,
      hljs.C_LINE_COMMENT_MODE,
      // Slashdash comments (comment out a node)
      { begin: /\/-/, end: /\n/ },
    ],
  };

  const NUMBERS: Mode = {
    scope: "number",
    variants: [
      // Binary
      { begin: /([+-])?0b[_01]+/ },
      // Octal
      { begin: /([+-])?0o[_0-7]+/ },
      // Hex
      { begin: /([+-])?0x[_0-9A-Fa-f]+/ },
      // Decimal (with optional fraction and exponent)
      { begin: /([+-])?[0-9][0-9_]*(\.[0-9_]+)?([eE][+-]?[0-9_]+)?/ },
    ],
  };

  const TYPE_ANNOTATIONS: Mode = {
    scope: "type",
    begin: /\(/,
    end: /\)/,
  };

  return {
    name: "KDL",
    aliases: ["kdl"],
    contains: [
      STRINGS,
      COMMENTS,
      NUMBERS,
      TYPE_ANNOTATIONS,
      // Node names at start of line
      {
        scope: "title.function",
        begin: /^\s*[a-zA-Z_][a-zA-Z0-9_-]*/,
      },
      // Property names (key=value)
      {
        scope: "attr",
        begin: /[a-zA-Z_][a-zA-Z0-9_-]*(?==)/,
      },
    ],
    keywords: {
      literal: ["true", "false", "null"],
    },
  };
}
