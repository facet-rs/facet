import * as monaco from 'monaco-editor';
import { initVimMode, VimMode } from 'monaco-vim';

// Styx Monarch grammar - line-based key/value distinction
const styxLanguage: monaco.languages.IMonarchLanguage = {
  defaultToken: 'invalid',
  tokenPostfix: '.styx',

  brackets: [
    { open: '{', close: '}', token: 'delimiter.curly' },
    { open: '(', close: ')', token: 'delimiter.parenthesis' },
  ],

  tokenizer: {
    root: [
      [/[ \t]+/, 'white'],
      [/\r?\n/, { token: 'white', next: '@root' }],
      [/\/\/\/.*$/, 'comment.doc'],
      [/\/\/.*$/, 'comment'],
      [/@[A-Za-z_][A-Za-z0-9_\-]*/, { token: 'tag', next: '@afterKey' }],
      [/@(?![A-Za-z_])/, { token: 'tag', next: '@afterKey' }],
      [/r#*"/, { token: 'string.key', next: '@rawString' }],
      [/"/, { token: 'string.key', next: '@stringKey' }],
      [/[{}]/, 'delimiter.curly'],
      [/[()]/, 'delimiter.parenthesis'],
      [/,/, 'delimiter.comma'],
      [/[^\s{}\(\),"=@<>\r\n]+/, { token: 'key', next: '@afterKey' }],
    ],

    afterKey: [
      [/[ \t]+/, 'white'],
      [/\r?\n/, { token: 'white', next: '@root' }],
      [/\/\/.*$/, 'comment'],
      [/@[A-Za-z_][A-Za-z0-9_\-]*/, 'tag'],
      [/@(?![A-Za-z_])/, 'tag'],
      [/r#*"/, { token: 'string', next: '@rawString' }],
      [/<<[A-Z][A-Z0-9_]*(,[a-z]+)?/, { token: 'string.heredoc', next: '@heredoc' }],
      [/"/, { token: 'string', next: '@string' }],
      [/[{}]/, 'delimiter.curly'],
      [/[()]/, 'delimiter.parenthesis'],
      [/,/, 'delimiter.comma'],
      [/[^\s{}\(\),"=@>\r\n]+>[^\s{}\(\),"\r\n]+/, 'attribute'],
      [/[^\s{}\(\),"=@<>\r\n]+/, 'value'],
    ],

    stringKey: [
      [/[^\\"]+/, 'string.key'],
      [/\\./, 'string.escape'],
      [/"/, { token: 'string.key', next: '@afterKey' }],
    ],

    string: [
      [/[^\\"]+/, 'string'],
      [/\\./, 'string.escape'],
      [/"/, { token: 'string', next: '@pop' }],
    ],

    rawString: [
      [/"#*/, { token: 'string', next: '@pop' }],
      [/[^"]+/, 'string'],
    ],

    heredoc: [
      [/^[A-Z][A-Z0-9_]*$/, { token: 'string.heredoc', next: '@root' }],
      [/.*$/, 'string.heredoc'],
    ],
  },
};

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

// OneDark-inspired theme
const styxDarkTheme: monaco.editor.IStandaloneThemeData = {
  base: 'vs-dark',
  inherit: true,
  rules: [
    { token: 'comment', foreground: '5c6370', fontStyle: 'italic' },
    { token: 'comment.doc', foreground: '7f848e', fontStyle: 'italic' },
    { token: 'tag', foreground: 'c678dd' },
    { token: 'key', foreground: 'e06c75' },
    { token: 'value', foreground: '61afef' },
    { token: 'string', foreground: '98c379' },
    { token: 'string.key', foreground: 'e06c75' },
    { token: 'string.heredoc', foreground: '98c379' },
    { token: 'string.escape', foreground: 'd19a66' },
    { token: 'attribute', foreground: '56b6c2' },
    { token: 'delimiter.curly', foreground: 'e5c07b' },
    { token: 'delimiter.parenthesis', foreground: 'c678dd' },
    { token: 'delimiter.comma', foreground: 'abb2bf' },
  ],
  colors: {
    'editor.background': '#282c34',
    'editor.foreground': '#abb2bf',
    'editor.lineHighlightBackground': '#2c313c',
    'editorCursor.foreground': '#528bff',
    'editor.selectionBackground': '#3e4451',
    'editorLineNumber.foreground': '#4b5263',
    'editorLineNumber.activeForeground': '#abb2bf',
  },
};

// Register language
export function registerStyxLanguage(): void {
  monaco.languages.register({ id: 'styx' });
  monaco.languages.setMonarchTokensProvider('styx', styxLanguage);
  monaco.languages.setLanguageConfiguration('styx', styxLanguageConfig);
  monaco.editor.defineTheme('styx-dark', styxDarkTheme);
}

// Export everything needed
export { monaco, initVimMode, VimMode };
