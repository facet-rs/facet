import { parser } from "./syntax.grammar";
import {
  LRLanguage,
  LanguageSupport,
  indentNodeProp,
  foldNodeProp,
  foldInside,
  delimitedIndent,
  syntaxHighlighting,
  HighlightStyle,
} from "@codemirror/language";
import { completeFromList } from "@codemirror/autocomplete";
import { tags as t } from "@lezer/highlight";

// Language definition with syntax highlighting
export const styxLanguage = LRLanguage.define({
  name: "styx",
  parser: parser,
  languageData: {
    commentTokens: { line: "//" },
    closeBrackets: { brackets: ["(", "{", '"'] },
  },
});

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

// Define Styx-specific highlighting style
const styxHighlightStyle = HighlightStyle.define([
  { tag: t.lineComment, color: "#6a9955" },
  { tag: t.docComment, color: "#6a9955", fontStyle: "italic" },
  { tag: t.string, color: "#ce9178" },
  { tag: t.special(t.string), color: "#d7ba7d" },
  { tag: t.tagName, color: "#569cd6" },
  { tag: t.attributeName, color: "#9cdcfe" },
  { tag: t.null, color: "#569cd6" },
  { tag: t.paren, color: "#ffd700" },
  { tag: t.brace, color: "#da70d6" },
  { tag: t.separator, color: "#d4d4d4" },
]);

// Syntax highlighting extension
const styxHighlightingExt = syntaxHighlighting(styxHighlightStyle);

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
 */
export function styx(): LanguageSupport {
  return new LanguageSupport(styxLanguage, [styxCompletion, styxHighlightingExt]);
}

// Re-export for advanced usage
export { parser } from "./syntax.grammar";
