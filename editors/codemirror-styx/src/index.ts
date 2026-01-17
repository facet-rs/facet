import { parser } from "./syntax.grammar";
import { LRLanguage, LanguageSupport } from "@codemirror/language";
import { completeFromList } from "@codemirror/autocomplete";

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
  return new LanguageSupport(styxLanguage, [styxCompletion]);
}

// Re-export for advanced usage
export { parser } from "./syntax.grammar";
