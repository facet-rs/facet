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
} from "@codemirror/language";
import { completeFromList } from "@codemirror/autocomplete";

// Custom fold service for Styx - finds Object/Sequence nodes and returns fold ranges
const styxFoldService = foldService.of((state, lineStart, lineEnd) => {
  const tree = syntaxTree(state);
  let node = tree.resolveInner(lineEnd, -1);

  // Walk up the tree looking for Object or Sequence
  for (let cur: typeof node | null = node; cur; cur = cur.parent) {
    if (cur.type.name === "Object" || cur.type.name === "Sequence") {
      const first = cur.firstChild;
      const last = cur.lastChild;
      // Only fold if it spans multiple lines
      if (first && last && first.to < last.from) {
        return { from: first.to, to: last.from };
      }
    }
  }
  return null;
});

// Language definition with syntax highlighting and code folding
// Using parser.configure() like @codemirror/lang-json does
export const styxLanguage = LRLanguage.define({
  name: "styx",
  parser: parser.configure({
    props: [
      indentNodeProp.add({
        Object: continuedIndent({ except: /^\s*\}/ }),
        Sequence: continuedIndent({ except: /^\s*\)/ }),
      }),
      foldNodeProp.add({
        Object: foldInside,
        Sequence: foldInside,
      }),
    ],
  }),
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
  return new LanguageSupport(styxLanguage, [styxCompletion, styxFoldService]);
}

// Re-export for advanced usage
export { parser } from "./syntax.grammar";
