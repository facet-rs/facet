import * as monaco from 'monaco-editor';
import { initVimMode, VimMode } from 'monaco-vim';
import { StyxTokensProvider } from './tokenizer';

const styxLanguageConfig: monaco.languages.LanguageConfiguration = {
  comments: { lineComment: '//' },
  brackets: [['{', '}'], ['(', ')']],
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

// Catppuccin Mocha theme - https://github.com/catppuccin/catppuccin
// Base colors
const mocha = {
  rosewater: 'f5e0dc',
  flamingo: 'f2cdcd',
  pink: 'f5c2e7',
  mauve: 'cba6f7',
  red: 'f38ba8',
  maroon: 'eba0ac',
  peach: 'fab387',
  yellow: 'f9e2af',
  green: 'a6e3a1',
  teal: '94e2d5',
  sky: '89dceb',
  sapphire: '74c7ec',
  blue: '89b4fa',
  lavender: 'b4befe',
  text: 'cdd6f4',
  subtext1: 'bac2de',
  subtext0: 'a6adc8',
  overlay2: '9399b2',
  overlay1: '7f849c',
  overlay0: '6c7086',
  surface2: '585b70',
  surface1: '45475a',
  surface0: '313244',
  base: '1e1e2e',
  mantle: '181825',
  crust: '11111b',
};

const catppuccinMocha: monaco.editor.IStandaloneThemeData = {
  base: 'vs-dark',
  inherit: true,
  rules: [
    // Comments
    { token: 'comment', foreground: mocha.overlay1, fontStyle: 'italic' },
    { token: 'comment.doc', foreground: mocha.overlay2, fontStyle: 'italic' },

    // Keys - pink/flamingo for that warm feel
    { token: 'key', foreground: mocha.flamingo },
    { token: 'string.key', foreground: mocha.flamingo },
    { token: 'tag.key', foreground: mocha.mauve },

    // Values - sapphire/blue
    { token: 'value', foreground: mocha.sapphire },

    // Tags - mauve (purple)
    { token: 'tag', foreground: mocha.mauve },

    // Strings - green
    { token: 'string', foreground: mocha.green },
    { token: 'string.heredoc', foreground: mocha.green },
    { token: 'string.escape', foreground: mocha.peach },

    // Delimiters
    { token: 'delimiter.curly', foreground: mocha.yellow },
    { token: 'delimiter.parenthesis', foreground: mocha.pink },
    { token: 'delimiter.comma', foreground: mocha.overlay2 },

    // Invalid
    { token: 'invalid', foreground: mocha.red },

    // Additional token types for embedded languages
    { token: 'keyword', foreground: mocha.mauve },
    { token: 'keyword.sql', foreground: mocha.mauve },
    { token: 'operator', foreground: mocha.sky },
    { token: 'operator.sql', foreground: mocha.sky },
    { token: 'number', foreground: mocha.peach },
    { token: 'number.json', foreground: mocha.peach },
    { token: 'identifier', foreground: mocha.text },
    { token: 'type', foreground: mocha.yellow },
    { token: 'type.identifier.json', foreground: mocha.blue },
    { token: 'predefined', foreground: mocha.blue },
    { token: 'predefined.sql', foreground: mocha.blue },

    // JSON specific
    { token: 'string.key.json', foreground: mocha.flamingo },
    { token: 'string.value.json', foreground: mocha.green },
    { token: 'keyword.json', foreground: mocha.mauve },
    { token: 'delimiter.bracket.json', foreground: mocha.overlay2 },
    { token: 'delimiter.array.json', foreground: mocha.pink },
    { token: 'delimiter.colon.json', foreground: mocha.overlay2 },
    { token: 'delimiter.comma.json', foreground: mocha.overlay2 },
  ],
  colors: {
    'editor.background': '#' + mocha.base,
    'editor.foreground': '#' + mocha.text,
    'editor.lineHighlightBackground': '#' + mocha.surface0,
    'editorCursor.foreground': '#' + mocha.rosewater,
    'editor.selectionBackground': '#' + mocha.surface2 + '80',
    'editorLineNumber.foreground': '#' + mocha.surface2,
    'editorLineNumber.activeForeground': '#' + mocha.lavender,
    'editorIndentGuide.background': '#' + mocha.surface1,
    'editorIndentGuide.activeBackground': '#' + mocha.surface2,
    'editorBracketMatch.background': '#' + mocha.surface2 + '80',
    'editorBracketMatch.border': '#' + mocha.mauve,
    'editor.findMatchBackground': '#' + mocha.peach + '40',
    'editor.findMatchHighlightBackground': '#' + mocha.yellow + '30',
    'editorWidget.background': '#' + mocha.mantle,
    'editorWidget.border': '#' + mocha.surface1,
    'input.background': '#' + mocha.surface0,
    'input.border': '#' + mocha.surface1,
    'input.foreground': '#' + mocha.text,
    'scrollbarSlider.background': '#' + mocha.surface1 + '80',
    'scrollbarSlider.hoverBackground': '#' + mocha.surface2 + '80',
    'scrollbarSlider.activeBackground': '#' + mocha.surface2,
  },
};

// Register language with custom tokenizer
export function registerStyxLanguage(): void {
  monaco.languages.register({ id: 'styx' });
  monaco.languages.setTokensProvider('styx', new StyxTokensProvider());
  monaco.languages.setLanguageConfiguration('styx', styxLanguageConfig);
  monaco.editor.defineTheme('catppuccin-mocha', catppuccinMocha);
}

// Export everything needed
export { monaco, initVimMode, VimMode };
