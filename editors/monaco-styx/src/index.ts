import type * as monaco from 'monaco-editor';
import { StyxTokensProvider } from './tokenizer';
import { catppuccinMocha, mocha } from './theme';

export { StyxTokensProvider } from './tokenizer';
export { catppuccinMocha, mocha } from './theme';

/**
 * Styx language configuration for Monaco editor.
 * Defines brackets, comments, and auto-closing pairs.
 */
export const styxLanguageConfig: monaco.languages.LanguageConfiguration = {
  comments: { lineComment: '//' },
  brackets: [
    ['{', '}'],
    ['(', ')'],
  ],
  autoClosingPairs: [
    { open: '{', close: '}' },
    { open: '(', close: ')' },
    { open: '"', close: '"' },
  ],
  surroundingPairs: [
    { open: '{', close: '}' },
    { open: '(', close: ')' },
    { open: '"', close: '"' },
  ],
};

/**
 * Options for registering the Styx language.
 */
export interface RegisterStyxOptions {
  /**
   * Whether to define the Catppuccin Mocha theme.
   * @default true
   */
  defineTheme?: boolean;

  /**
   * Whether to register embedded languages for heredoc injection (SQL, JavaScript, etc.)
   * @default true
   */
  registerEmbeddedLanguages?: boolean;
}

/**
 * Language definitions for embedded language support in heredocs.
 */
interface EmbeddedLanguage {
  id: string;
  def: {
    conf: monaco.languages.LanguageConfiguration;
    language: monaco.languages.IMonarchLanguage;
  };
}

/**
 * Register the Styx language with Monaco editor.
 *
 * @param monacoInstance - The monaco module
 * @param embeddedLanguages - Optional array of embedded languages for heredoc injection.
 *                            These are the monaco basic language definitions (sql, javascript, etc.)
 * @param options - Registration options
 *
 * @example
 * ```ts
 * import * as monaco from 'monaco-editor';
 * import * as sqlLang from 'monaco-editor/esm/vs/basic-languages/sql/sql';
 * import { registerStyxLanguage } from '@bearcove/monaco-lang-styx';
 *
 * registerStyxLanguage(monaco, [
 *   { id: 'sql', def: sqlLang },
 * ]);
 * ```
 */
export function registerStyxLanguage(
  monacoInstance: typeof monaco,
  embeddedLanguages?: EmbeddedLanguage[],
  options: RegisterStyxOptions = {}
): void {
  const { defineTheme = true, registerEmbeddedLanguages = true } = options;

  // Register embedded languages first (for heredoc injection)
  if (registerEmbeddedLanguages && embeddedLanguages) {
    for (const { id, def } of embeddedLanguages) {
      // Check if language is already registered
      const existing = monacoInstance.languages.getLanguages().find((l) => l.id === id);
      if (!existing) {
        monacoInstance.languages.register({ id });
      }
      monacoInstance.languages.setMonarchTokensProvider(id, def.language);
      monacoInstance.languages.setLanguageConfiguration(id, def.conf);
    }
  }

  // Register Styx language
  monacoInstance.languages.register({ id: 'styx' });
  monacoInstance.languages.setTokensProvider('styx', new StyxTokensProvider(monacoInstance.editor));
  monacoInstance.languages.setLanguageConfiguration('styx', styxLanguageConfig);

  // Define theme
  if (defineTheme) {
    monacoInstance.editor.defineTheme('catppuccin-mocha', catppuccinMocha);
  }
}
