import * as monaco from 'monaco-editor';
import { initVimMode, VimMode } from 'monaco-vim';

// Styx Monarch grammar
// Based on the Styx parser spec:
// - Entry = key + optional value (1 or 2 atoms)
// - In object context: first atom is KEY, second is VALUE
// - In sequence context: everything is VALUE
// - { } delimit objects, ( ) delimit sequences

const styxLanguage: monaco.languages.IMonarchLanguage = {
  defaultToken: 'invalid',
  tokenPostfix: '.styx',

  brackets: [
    { open: '{', close: '}', token: 'delimiter.curly' },
    { open: '(', close: ')', token: 'delimiter.parenthesis' },
  ],

  tokenizer: {
    // Root = object context, expecting entries (key + optional value)
    root: [
      [/[ \t]+/, 'white'],
      [/\r?\n/, 'white'],
      [/\/\/\/.*$/, 'comment.doc'],
      [/\/\/.*$/, 'comment'],

      // Braces - { starts new object context, } ends current
      [/\{/, { token: 'delimiter.curly', next: '@push' }],
      [/\}/, { token: 'delimiter.curly', next: '@pop' }],

      // Parentheses - start sequence context
      [/\(/, { token: 'delimiter.parenthesis', next: '@sequence' }],
      [/\)/, 'delimiter.parenthesis'],

      [/,/, 'delimiter.comma'],

      // === KEY patterns (first atom of entry) ===

      // Tag as key with immediate payload (no space)
      // @tag{...} - tag + object payload
      [/@[A-Za-z_][A-Za-z0-9_\-]*(?=\{)/, 'tag.key'],
      // @tag(...) - tag + sequence payload
      [/@[A-Za-z_][A-Za-z0-9_\-]*(?=\()/, 'tag.key'],
      // @tag"..." - tag + string payload
      [/@[A-Za-z_][A-Za-z0-9_\-]*(?=")/, { token: 'tag.key', next: '@tagStringKey' }],
      [/@[A-Za-z_][A-Za-z0-9_\-]*(?=r#*")/, { token: 'tag.key', next: '@tagRawStringKey' }],
      // @tag or @ alone (unit) as key
      [/@[A-Za-z_][A-Za-z0-9_\-]*/, { token: 'tag.key', next: '@afterKey' }],
      [/@(?![A-Za-z_])/, { token: 'tag.key', next: '@afterKey' }],

      // Quoted string as key
      [/"/, { token: 'string.key', next: '@stringKey' }],
      [/r(#*)"/, { token: 'string.key', next: '@rawStringKey.$1' }],

      // Bare key - careful with characters
      // First char: not whitespace, { } ( ) , " = @ >
      // Subsequent: @ and = allowed, > still forbidden (for attributes), . terminates (for paths)
      [/[^\s{}\(\),"=@>\r\n][^\s{}\(\),">\r\n]*/, { token: 'key', next: '@afterKey' }],
    ],

    // After seeing a key, expect value or end of entry
    afterKey: [
      [/[ \t]+/, 'white'],
      // Newline ends entry, back to root for next key
      [/\r?\n/, { token: 'white', next: '@root' }],
      [/\/\/.*$/, 'comment'],

      // Comma ends entry, back to root for next key
      [/,/, { token: 'delimiter.comma', next: '@root' }],

      // { starts object value (nested object context)
      [/\{/, { token: 'delimiter.curly', next: '@root' }],
      // } closes enclosing object, back to root
      [/\}/, { token: 'delimiter.curly', next: '@root' }],

      // ( starts sequence value
      [/\(/, { token: 'delimiter.parenthesis', next: '@sequence' }],
      // ) closes sequence (shouldn't happen here normally)
      [/\)/, { token: 'delimiter.parenthesis', next: '@root' }],

      // Heredoc as value - with optional language hint for injection
      // <<DELIM or <<DELIM,lang
      [/<<([A-Z][A-Z0-9_]*),([a-z][a-z0-9_.\-]*)/, { token: 'string.heredoc', next: '@heredocLang.$1.$2', nextEmbedded: '$2' }],
      [/<<([A-Z][A-Z0-9_]*)/, { token: 'string.heredoc', next: '@heredoc.$1' }],

      // === VALUE patterns (second atom of entry) ===

      // Tag as value with immediate payload (payload will handle return to root)
      [/@[A-Za-z_][A-Za-z0-9_\-]*(?=\{)/, 'tag'],
      [/@[A-Za-z_][A-Za-z0-9_\-]*(?=\()/, 'tag'],
      [/@[A-Za-z_][A-Za-z0-9_\-]*(?=")/, { token: 'tag', next: '@tagString' }],
      [/@[A-Za-z_][A-Za-z0-9_\-]*(?=r#*")/, { token: 'tag', next: '@tagRawString' }],
      // @tag or @ alone (unit) as value - entry complete
      [/@[A-Za-z_][A-Za-z0-9_\-]*/, { token: 'tag', next: '@root' }],
      [/@(?![A-Za-z_])/, { token: 'tag', next: '@root' }],

      // Quoted string as value
      [/"/, { token: 'string', next: '@string' }],
      [/r(#*)"/, { token: 'string', next: '@rawString.$1' }],

      // Attribute syntax: key>value (creates inline object) - entry complete
      [/[^\s{}\(\),"=@>\r\n]+>[^\s{}\(\),"\r\n]*/, { token: 'value', next: '@root' }],

      // Bare value - entry complete, back to root for next key
      [/[^\s{}\(\),"=@>\r\n][^\s{}\(\),">\r\n]*/, { token: 'value', next: '@root' }],
    ],

    // Sequence context - everything is a value until )
    sequence: [
      [/[ \t\r\n]+/, 'white'],
      [/\/\/.*$/, 'comment'],

      // Nested sequence
      [/\(/, { token: 'delimiter.parenthesis', next: '@push' }],
      [/\)/, { token: 'delimiter.parenthesis', next: '@pop' }],

      // Object inside sequence
      [/\{/, { token: 'delimiter.curly', next: '@root' }],
      [/\}/, 'delimiter.curly'],

      // Tags in sequence (all values)
      [/@[A-Za-z_][A-Za-z0-9_\-]*(?=\{)/, 'tag'],
      [/@[A-Za-z_][A-Za-z0-9_\-]*(?=\()/, 'tag'],
      [/@[A-Za-z_][A-Za-z0-9_\-]*(?=")/, { token: 'tag', next: '@tagString' }],
      [/@[A-Za-z_][A-Za-z0-9_\-]*(?=r#*")/, { token: 'tag', next: '@tagRawString' }],
      [/@[A-Za-z_][A-Za-z0-9_\-]*/, 'tag'],
      [/@(?![A-Za-z_])/, 'tag'],

      // Strings
      [/"/, { token: 'string', next: '@string' }],
      [/r(#*)"/, { token: 'string', next: '@rawString.$1' }],

      // Values
      [/[^\s{}\(\),"=@>\r\n][^\s{}\(\),">\r\n]*/, 'value'],
    ],

    // String as key (quoted)
    stringKey: [
      [/[^\\"]+/, 'string.key'],
      [/\\./, 'string.escape'],
      [/"/, { token: 'string.key', next: '@afterKey' }],
    ],

    // Raw string as key
    'rawStringKey.$S2': [
      [/"(#*)/, {
        cases: {
          '$1==$S2': { token: 'string.key', next: '@afterKey' },
          '@default': 'string.key'
        }
      }],
      [/[^"]+/, 'string.key'],
      [/"/, 'string.key'],
    ],

    // Tag with string payload as key
    tagStringKey: [
      [/"/, { token: 'string.key', next: '@stringKey' }],
    ],

    // Tag with raw string payload as key
    tagRawStringKey: [
      [/r(#*)"/, { token: 'string.key', next: '@rawStringKey.$1' }],
    ],

    // String as value (quoted) - return to root when done
    string: [
      [/[^\\"]+/, 'string'],
      [/\\./, 'string.escape'],
      [/"/, { token: 'string', next: '@root' }],
    ],

    // Raw string as value - return to root when done
    'rawString.$S2': [
      [/"(#*)/, {
        cases: {
          '$1==$S2': { token: 'string', next: '@root' },
          '@default': 'string'
        }
      }],
      [/[^"]+/, 'string'],
      [/"/, 'string'],
    ],

    // Tag with string payload as value
    tagString: [
      [/"/, { token: 'string', next: '@string' }],
    ],

    // Tag with raw string payload as value
    tagRawString: [
      [/r(#*)"/, { token: 'string', next: '@rawString.$1' }],
    ],

    // Heredoc
    'heredoc.$S2': [
      [/^(\s*)(\S+)$/, {
        cases: {
          '$2==$S2': { token: 'string.heredoc', next: '@pop' },
          '@default': 'string.heredoc'
        }
      }],
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

// OneDark-inspired theme with clear key/value distinction
const styxDarkTheme: monaco.editor.IStandaloneThemeData = {
  base: 'vs-dark',
  inherit: true,
  rules: [
    { token: 'comment', foreground: '5c6370', fontStyle: 'italic' },
    { token: 'comment.doc', foreground: '7f848e', fontStyle: 'italic' },

    // Keys - red/coral (like object keys in most themes)
    { token: 'key', foreground: 'e06c75' },
    { token: 'string.key', foreground: 'e06c75' },
    { token: 'tag.key', foreground: 'c678dd' },

    // Values - blue (like string/number values)
    { token: 'value', foreground: '61afef' },

    // Tags - purple
    { token: 'tag', foreground: 'c678dd' },

    // Strings - green
    { token: 'string', foreground: '98c379' },
    { token: 'string.heredoc', foreground: '98c379' },
    { token: 'string.escape', foreground: 'd19a66' },

    // Delimiters
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
