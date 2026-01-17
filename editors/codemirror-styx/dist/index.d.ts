import { LRLanguage, LanguageSupport } from '@codemirror/language';
import { LRParser } from '@lezer/lr';

declare const parser: LRParser;

declare const styxLanguage: LRLanguage;
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
declare function styx(): LanguageSupport;

export { parser, styx, styxLanguage };
