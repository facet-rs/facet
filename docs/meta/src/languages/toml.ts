/**
 * TOML language definition for highlight.js
 * Based on https://toml.io specification
 */

import type { HLJSApi, Language, Mode } from "highlight.js";

export default function toml(hljs: HLJSApi): Language {
  const BARE_KEY = /[A-Za-z0-9_-]+/;

  const STRINGS: Mode = {
    scope: "string",
    variants: [
      // Multi-line basic string
      { begin: /"""/, end: /"""/ },
      // Multi-line literal string
      { begin: /'''/, end: /'''/ },
      // Basic string
      { begin: /"/, end: /"/, illegal: /\n/ },
      // Literal string
      { begin: /'/, end: /'/, illegal: /\n/ },
    ],
  };

  const NUMBERS: Mode = {
    scope: "number",
    variants: [
      // Hex
      { begin: /0x[0-9A-Fa-f_]+/ },
      // Octal
      { begin: /0o[0-7_]+/ },
      // Binary
      { begin: /0b[01_]+/ },
      // Special floats
      { begin: /[+-]?(inf|nan)/ },
      // Decimal (with optional fraction and exponent)
      { begin: /[+-]?[0-9][0-9_]*(\.[0-9_]+)?([eE][+-]?[0-9_]+)?/ },
    ],
  };

  const DATETIME: Mode = {
    scope: "number",
    // ISO 8601 dates and times
    begin:
      /\d{4}-\d{2}-\d{2}([T ]\d{2}:\d{2}:\d{2}(\.\d+)?(Z|[+-]\d{2}:\d{2})?)?/,
  };

  return {
    name: "TOML",
    aliases: ["toml"],
    contains: [
      hljs.HASH_COMMENT_MODE,
      // Table headers: [table] or [[array-of-tables]]
      {
        scope: "section",
        begin: /^\s*\[\[?/,
        end: /\]\]?/,
        contains: [{ scope: "variable", begin: BARE_KEY }],
      },
      // Keys
      {
        scope: "attr",
        begin: BARE_KEY,
        end: /\s*=/,
        excludeEnd: true,
      },
      STRINGS,
      NUMBERS,
      DATETIME,
      // Booleans
      {
        scope: "literal",
        begin: /\b(true|false)\b/,
      },
    ],
  };
}
