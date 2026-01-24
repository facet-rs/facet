import * as monaco from 'monaco-editor';

/**
 * Monaco tokens provider for Styx language.
 * Handles context-aware tokenization including heredocs and embedded language injection.
 */
declare class StyxTokensProvider implements monaco.languages.TokensProvider {
    private monacoEditor;
    /**
     * @param monacoEditor Optional monaco.editor reference for embedded language tokenization.
     *                     If not provided, heredocs will be styled as plain heredoc strings.
     */
    constructor(monacoEditor?: typeof monaco.editor);
    getInitialState(): monaco.languages.IState;
    tokenize(line: string, inputState: monaco.languages.IState): monaco.languages.ILineTokens;
}

declare const mocha: {
    rosewater: string;
    flamingo: string;
    pink: string;
    mauve: string;
    red: string;
    maroon: string;
    peach: string;
    yellow: string;
    green: string;
    teal: string;
    sky: string;
    sapphire: string;
    blue: string;
    lavender: string;
    text: string;
    subtext1: string;
    subtext0: string;
    overlay2: string;
    overlay1: string;
    overlay0: string;
    surface2: string;
    surface1: string;
    surface0: string;
    base: string;
    mantle: string;
    crust: string;
};
/**
 * Catppuccin Mocha theme for Monaco editor.
 * A dark theme with warm, readable colors optimized for Styx syntax highlighting.
 */
declare const catppuccinMocha: monaco.editor.IStandaloneThemeData;

/**
 * Styx language configuration for Monaco editor.
 * Defines brackets, comments, and auto-closing pairs.
 */
declare const styxLanguageConfig: monaco.languages.LanguageConfiguration;
/**
 * Options for registering the Styx language.
 */
interface RegisterStyxOptions {
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
declare function registerStyxLanguage(monacoInstance: typeof monaco, embeddedLanguages?: EmbeddedLanguage[], options?: RegisterStyxOptions): void;

export { type RegisterStyxOptions, StyxTokensProvider, catppuccinMocha, mocha, registerStyxLanguage, styxLanguageConfig };
