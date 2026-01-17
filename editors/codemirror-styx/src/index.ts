import { parser } from "./syntax.grammar";
import {
  LRLanguage,
  LanguageSupport,
  foldNodeProp,
  foldInside,
  indentNodeProp,
  continuedIndent,
  syntaxTree,
  foldService,
  Language,
} from "@codemirror/language";
import { completeFromList } from "@codemirror/autocomplete";
import { parseMixed } from "@lezer/common";
import type { SyntaxNodeRef, Input, NestedParse, Parser } from "@lezer/common";

/**
 * Configuration for nested languages in heredocs.
 * Maps language hints (e.g., "sql", "json") to CodeMirror LanguageSupport objects.
 */
export interface NestedLanguage {
  /** Language hint as it appears after comma in heredoc (e.g., "sql" in <<SQL,sql) */
  tag: string;
  /** The CodeMirror LanguageSupport to use for parsing */
  language: LanguageSupport;
}

/**
 * Parse heredoc text to extract marker info.
 * Returns { delimiter, langHint, contentStart, contentEnd }
 */
function parseHeredocText(text: string): {
  delimiter: string;
  langHint: string | null;
  contentStart: number;
  contentEnd: number;
} | null {
  // Format: <<DELIM[,lang]\n...content...\nDELIM
  const match = text.match(/^<<([A-Z][A-Z0-9_]*)(?:,([a-z][a-z0-9_.-]*))?\r?\n/);
  if (!match) return null;

  const delimiter = match[1];
  const langHint = match[2] || null;
  const headerLen = match[0].length;

  // Find where the closing delimiter starts
  const delimPattern = new RegExp(`^[ \\t]*${delimiter}$`, "m");
  const contentMatch = text.slice(headerLen).match(delimPattern);

  if (!contentMatch || contentMatch.index === undefined) {
    // No closing delimiter found - content goes to end
    return {
      delimiter,
      langHint,
      contentStart: headerLen,
      contentEnd: text.length,
    };
  }

  return {
    delimiter,
    langHint,
    contentStart: headerLen,
    contentEnd: headerLen + contentMatch.index,
  };
}

/**
 * Creates a parser wrapper that handles nested language injection for heredocs.
 */
function createMixedParser(nestedLanguages: NestedLanguage[]) {
  const langMap = new Map<string, Parser>();
  for (const { tag, language } of nestedLanguages) {
    langMap.set(tag, language.language.parser);
  }

  return parseMixed((node: SyntaxNodeRef, input: Input): NestedParse | null => {
    if (node.type.name !== "Heredoc") return null;

    // Get the heredoc text
    const text = input.read(node.from, node.to);
    const parsed = parseHeredocText(text);

    if (!parsed || !parsed.langHint) return null;

    // Find the parser for this language hint
    const nestedParser = langMap.get(parsed.langHint);
    if (!nestedParser) return null;

    // Return overlay for just the content portion
    return {
      parser: nestedParser,
      overlay: [{ from: node.from + parsed.contentStart, to: node.from + parsed.contentEnd }],
    };
  });
}

// Custom fold service for Styx - finds Object/Sequence nodes and returns fold ranges
const styxFoldService = foldService.of((state, lineStart, lineEnd) => {
  const tree = syntaxTree(state);
  let node = tree.resolveInner(lineEnd, -1);

  // Walk up the tree looking for Object or Sequence
  for (let cur: typeof node | null = node; cur; cur = cur.parent) {
    if (cur.type.name === "Object" || cur.type.name === "Sequence") {
      const first = cur.firstChild;
      const last = cur.lastChild;
      // Only fold if:
      // 1. It spans multiple lines (first.to < last.from)
      // 2. The opening brace is on THIS line (first.from >= lineStart)
      if (first && last && first.to < last.from && first.from >= lineStart) {
        return { from: first.to, to: last.from };
      }
    }
  }
  return null;
});

// Base parser props
const baseProps = [
  indentNodeProp.add({
    Object: continuedIndent({ except: /^\s*\}/ }),
    Sequence: continuedIndent({ except: /^\s*\)/ }),
  }),
  foldNodeProp.add({
    Object: foldInside,
    Sequence: foldInside,
  }),
];

// Language definition with syntax highlighting and code folding
// Using parser.configure() like @codemirror/lang-json does
export const styxLanguage = LRLanguage.define({
  name: "styx",
  parser: parser.configure({ props: baseProps }),
  languageData: {
    commentTokens: { line: "//" },
    closeBrackets: { brackets: ["(", "{", '"'] },
  },
});

/**
 * Create a Styx language with nested language support for heredocs.
 */
function createStyxLanguage(nestedLanguages: NestedLanguage[]): LRLanguage {
  if (nestedLanguages.length === 0) {
    return styxLanguage;
  }

  const mixedParser = parser.configure({
    props: baseProps,
    wrap: createMixedParser(nestedLanguages),
  });

  return LRLanguage.define({
    name: "styx",
    parser: mixedParser,
    languageData: {
      commentTokens: { line: "//" },
      closeBrackets: { brackets: ["(", "{", '"'] },
    },
  });
}

// Common Styx schema tags for autocompletion
const builtinTags = [
  "@string",
  "@int",
  "@float",
  "@bool",
  "@null",
  "@object",
  "@array",
  "@optional",
  "@required",
  "@default",
  "@enum",
  "@pattern",
  "@min",
  "@max",
  "@minLength",
  "@maxLength",
].map((label) => ({ label, type: "keyword" }));

// Basic autocompletion for tags
const styxCompletion = styxLanguage.data.of({
  autocomplete: completeFromList(builtinTags),
});

/**
 * Configuration options for Styx language support.
 */
export interface StyxConfig {
  /**
   * Nested languages for heredoc content.
   * Maps language hints to CodeMirror Language objects.
   *
   * Example:
   * ```ts
   * import { sql } from "@codemirror/lang-sql";
   *
   * styx({
   *   nestedLanguages: [
   *     { tag: "sql", language: sql() }
   *   ]
   * })
   * ```
   */
  nestedLanguages?: NestedLanguage[];
}

/**
 * Styx language support for CodeMirror 6.
 *
 * Usage:
 * ```ts
 * import { styx } from "@bearcove/codemirror-lang-styx";
 * import { EditorView, basicSetup } from "codemirror";
 *
 * new EditorView({
 *   extensions: [basicSetup, styx()],
 *   parent: document.body,
 * });
 * ```
 *
 * With nested language support:
 * ```ts
 * import { sql } from "@codemirror/lang-sql";
 *
 * new EditorView({
 *   extensions: [basicSetup, styx({ nestedLanguages: [{ tag: "sql", language: sql() }] })],
 *   parent: document.body,
 * });
 * ```
 */
export function styx(config: StyxConfig = {}): LanguageSupport {
  const nestedLanguages = config.nestedLanguages || [];
  const lang = createStyxLanguage(nestedLanguages);

  // Collect nested language supports for proper extension loading
  const nestedSupports = nestedLanguages.flatMap((n) => n.language.support);

  return new LanguageSupport(lang, [styxCompletion, styxFoldService, ...nestedSupports]);
}

// Re-export for advanced usage
export { parser } from "./syntax.grammar";
