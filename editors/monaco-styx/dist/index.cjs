"use strict";
var __defProp = Object.defineProperty;
var __getOwnPropDesc = Object.getOwnPropertyDescriptor;
var __getOwnPropNames = Object.getOwnPropertyNames;
var __hasOwnProp = Object.prototype.hasOwnProperty;
var __export = (target, all) => {
  for (var name in all)
    __defProp(target, name, { get: all[name], enumerable: true });
};
var __copyProps = (to, from, except, desc) => {
  if (from && typeof from === "object" || typeof from === "function") {
    for (let key of __getOwnPropNames(from))
      if (!__hasOwnProp.call(to, key) && key !== except)
        __defProp(to, key, { get: () => from[key], enumerable: !(desc = __getOwnPropDesc(from, key)) || desc.enumerable });
  }
  return to;
};
var __toCommonJS = (mod) => __copyProps(__defProp({}, "__esModule", { value: true }), mod);

// src/index.ts
var index_exports = {};
__export(index_exports, {
  StyxTokensProvider: () => StyxTokensProvider,
  catppuccinMocha: () => catppuccinMocha,
  mocha: () => mocha,
  registerStyxLanguage: () => registerStyxLanguage,
  styxLanguageConfig: () => styxLanguageConfig
});
module.exports = __toCommonJS(index_exports);

// src/tokenizer.ts
function createInitialState() {
  return {
    contextStack: ["object"],
    // document is implicit root object
    entryPhase: "key",
    heredoc: null,
    rawStringHashes: null,
    inString: false,
    stringIsKey: false,
    clone() {
      return {
        contextStack: [...this.contextStack],
        entryPhase: this.entryPhase,
        heredoc: this.heredoc ? { ...this.heredoc } : null,
        rawStringHashes: this.rawStringHashes,
        inString: this.inString,
        stringIsKey: this.stringIsKey,
        clone: this.clone,
        equals: this.equals
      };
    },
    equals(other) {
      const o = other;
      return this.contextStack.length === o.contextStack.length && this.contextStack.every((v, i) => v === o.contextStack[i]) && this.entryPhase === o.entryPhase && JSON.stringify(this.heredoc) === JSON.stringify(o.heredoc) && this.rawStringHashes === o.rawStringHashes && this.inString === o.inString && this.stringIsKey === o.stringIsKey;
    }
  };
}
function currentContext(state) {
  return state.contextStack[state.contextStack.length - 1] || "object";
}
function isInObjectContext(state) {
  return currentContext(state) === "object";
}
var TOKEN = {
  WHITE: "white",
  COMMENT: "comment",
  COMMENT_DOC: "comment.doc",
  KEY: "key",
  STRING_KEY: "string.key",
  TAG_KEY: "tag.key",
  VALUE: "value",
  TAG: "tag",
  STRING: "string",
  STRING_HEREDOC: "string.heredoc",
  STRING_ESCAPE: "string.escape",
  DELIMITER_CURLY: "delimiter.curly",
  DELIMITER_PAREN: "delimiter.parenthesis",
  DELIMITER_COMMA: "delimiter.comma",
  INVALID: "invalid"
};
var WHITESPACE = /^[ \t]+/;
var DOC_COMMENT = /^\/\/\/.*/;
var LINE_COMMENT = /^\/\/.*/;
var TAG_IDENT = /^@[A-Za-z_][A-Za-z0-9_-]*/;
var UNIT = /^@(?![A-Za-z_])/;
var HEREDOC_START = /^<<([A-Z][A-Z0-9_]*)(?:,([a-z][a-z0-9_.-]*))?/;
var RAW_STRING_START = /^r(#+)"/;
var BARE_FIRST_CHAR = /^[^\s{}()\,\"=@>\r\n]/;
var BARE_CONT_CHAR = /^[^\s{}()\,\">\r\n]/;
var StyxTokensProvider = class {
  /**
   * @param monacoEditor Optional monaco.editor reference for embedded language tokenization.
   *                     If not provided, heredocs will be styled as plain heredoc strings.
   */
  constructor(monacoEditor) {
    this.monacoEditor = monacoEditor;
  }
  getInitialState() {
    return createInitialState();
  }
  tokenize(line, inputState) {
    const state = inputState.clone();
    const tokens = [];
    let pos = 0;
    const addToken = (start, type) => {
      tokens.push({ startIndex: start, scopes: type });
    };
    const atomType = (isTag) => {
      if (!isInObjectContext(state)) {
        return isTag ? TOKEN.TAG : TOKEN.VALUE;
      }
      if (state.entryPhase === "key") {
        return isTag ? TOKEN.TAG_KEY : TOKEN.KEY;
      }
      return isTag ? TOKEN.TAG : TOKEN.VALUE;
    };
    const stringType = () => {
      if (!isInObjectContext(state)) {
        return TOKEN.STRING;
      }
      return state.entryPhase === "key" ? TOKEN.STRING_KEY : TOKEN.STRING;
    };
    const afterAtom = () => {
      if (isInObjectContext(state)) {
        if (state.entryPhase === "key") {
          state.entryPhase = "value";
        } else {
          state.entryPhase = "key";
        }
      }
    };
    if (state.heredoc) {
      const delim = state.heredoc.delimiter;
      const closeMatch = line.match(new RegExp(`^(\\s*)(${delim})\\s*$`));
      if (closeMatch) {
        addToken(0, TOKEN.STRING_HEREDOC);
        state.heredoc = null;
        afterAtom();
        return { tokens, endState: state };
      }
      const lang = state.heredoc.language;
      if (lang && this.monacoEditor) {
        try {
          const embeddedTokens = this.monacoEditor.tokenize(line, lang);
          if (embeddedTokens.length > 0 && embeddedTokens[0].length > 0) {
            for (const token of embeddedTokens[0]) {
              tokens.push({
                startIndex: token.offset,
                scopes: token.type
              });
            }
            return { tokens, endState: state };
          }
        } catch {
        }
      }
      addToken(0, TOKEN.STRING_HEREDOC);
      return { tokens, endState: state };
    }
    if (state.inString) {
      const tokenType = state.stringIsKey ? TOKEN.STRING_KEY : TOKEN.STRING;
      while (pos < line.length) {
        const ch = line[pos];
        if (ch === "\\" && pos + 1 < line.length) {
          if (tokens.length === 0 || tokens[tokens.length - 1].startIndex !== pos) {
            addToken(pos, TOKEN.STRING_ESCAPE);
          }
          pos += 2;
          if (pos < line.length) {
            addToken(pos, tokenType);
          }
        } else if (ch === '"') {
          addToken(pos, tokenType);
          pos++;
          state.inString = false;
          afterAtom();
          break;
        } else {
          if (tokens.length === 0) {
            addToken(pos, tokenType);
          }
          pos++;
        }
      }
      if (pos >= line.length && state.inString) {
        if (tokens.length === 0) {
          addToken(0, tokenType);
        }
        return { tokens, endState: state };
      }
    }
    if (state.rawStringHashes !== null) {
      const hashes = state.rawStringHashes;
      const tokenType = state.stringIsKey ? TOKEN.STRING_KEY : TOKEN.STRING;
      const closePattern = '"' + "#".repeat(hashes);
      while (pos < line.length) {
        const idx = line.indexOf(closePattern, pos);
        if (idx >= 0) {
          addToken(pos, tokenType);
          pos = idx + closePattern.length;
          state.rawStringHashes = null;
          afterAtom();
          break;
        } else {
          addToken(pos, tokenType);
          return { tokens, endState: state };
        }
      }
    }
    while (pos < line.length) {
      const rest = line.slice(pos);
      let match;
      if (match = rest.match(WHITESPACE)) {
        addToken(pos, TOKEN.WHITE);
        pos += match[0].length;
        continue;
      }
      if (match = rest.match(DOC_COMMENT)) {
        addToken(pos, TOKEN.COMMENT_DOC);
        pos += match[0].length;
        continue;
      }
      if (match = rest.match(LINE_COMMENT)) {
        addToken(pos, TOKEN.COMMENT);
        pos += match[0].length;
        continue;
      }
      if (rest[0] === "{") {
        addToken(pos, TOKEN.DELIMITER_CURLY);
        pos++;
        state.contextStack.push("object");
        state.entryPhase = "key";
        continue;
      }
      if (rest[0] === "}") {
        addToken(pos, TOKEN.DELIMITER_CURLY);
        pos++;
        state.contextStack.pop();
        if (isInObjectContext(state) && state.entryPhase === "value") {
          state.entryPhase = "key";
        }
        continue;
      }
      if (rest[0] === "(") {
        addToken(pos, TOKEN.DELIMITER_PAREN);
        pos++;
        state.contextStack.push("sequence");
        continue;
      }
      if (rest[0] === ")") {
        addToken(pos, TOKEN.DELIMITER_PAREN);
        pos++;
        state.contextStack.pop();
        if (isInObjectContext(state) && state.entryPhase === "value") {
          state.entryPhase = "key";
        }
        continue;
      }
      if (rest[0] === ",") {
        addToken(pos, TOKEN.DELIMITER_COMMA);
        pos++;
        if (isInObjectContext(state)) {
          state.entryPhase = "key";
        }
        continue;
      }
      if (match = rest.match(HEREDOC_START)) {
        addToken(pos, TOKEN.STRING_HEREDOC);
        pos += match[0].length;
        state.heredoc = {
          delimiter: match[1],
          language: match[2] || null,
          indentation: null
        };
        return { tokens, endState: state };
      }
      if (match = rest.match(RAW_STRING_START)) {
        const hashes = match[1].length;
        const isKey = isInObjectContext(state) && state.entryPhase === "key";
        addToken(pos, isKey ? TOKEN.STRING_KEY : TOKEN.STRING);
        pos += match[0].length;
        const closePattern = '"' + "#".repeat(hashes);
        const closeIdx = line.indexOf(closePattern, pos);
        if (closeIdx >= 0) {
          pos = closeIdx + closePattern.length;
          afterAtom();
        } else {
          state.rawStringHashes = hashes;
          state.stringIsKey = isKey;
          return { tokens, endState: state };
        }
        continue;
      }
      if (rest[0] === '"') {
        const isKey = isInObjectContext(state) && state.entryPhase === "key";
        const tokenType = isKey ? TOKEN.STRING_KEY : TOKEN.STRING;
        addToken(pos, tokenType);
        pos++;
        while (pos < line.length) {
          const ch = line[pos];
          if (ch === "\\" && pos + 1 < line.length) {
            addToken(pos, TOKEN.STRING_ESCAPE);
            pos += 2;
            if (pos < line.length && line[pos] !== '"') {
              addToken(pos, tokenType);
            }
          } else if (ch === '"') {
            addToken(pos, tokenType);
            pos++;
            afterAtom();
            break;
          } else {
            pos++;
          }
        }
        if (pos >= line.length && line[line.length - 1] !== '"') {
          state.inString = true;
          state.stringIsKey = isKey;
        }
        continue;
      }
      if (match = rest.match(UNIT)) {
        addToken(pos, atomType(true));
        pos += match[0].length;
        afterAtom();
        continue;
      }
      if (match = rest.match(TAG_IDENT)) {
        addToken(pos, atomType(true));
        pos += match[0].length;
        const afterTag = line.slice(pos);
        if (afterTag[0] === "{" || afterTag[0] === "(") {
          continue;
        } else if (afterTag[0] === '"' || afterTag.match(/^r#+"/)) {
          continue;
        } else if (afterTag.match(/^<<[A-Z]/)) {
          continue;
        }
        afterAtom();
        continue;
      }
      if (rest.match(BARE_FIRST_CHAR)) {
        const startPos = pos;
        pos++;
        while (pos < line.length && line.slice(pos).match(BARE_CONT_CHAR)) {
          pos++;
        }
        addToken(startPos, atomType(false));
        afterAtom();
        continue;
      }
      addToken(pos, TOKEN.INVALID);
      pos++;
    }
    if (isInObjectContext(state)) {
      state.entryPhase = "key";
    }
    return { tokens, endState: state };
  }
};

// src/theme.ts
var mocha = {
  rosewater: "f5e0dc",
  flamingo: "f2cdcd",
  pink: "f5c2e7",
  mauve: "cba6f7",
  red: "f38ba8",
  maroon: "eba0ac",
  peach: "fab387",
  yellow: "f9e2af",
  green: "a6e3a1",
  teal: "94e2d5",
  sky: "89dceb",
  sapphire: "74c7ec",
  blue: "89b4fa",
  lavender: "b4befe",
  text: "cdd6f4",
  subtext1: "bac2de",
  subtext0: "a6adc8",
  overlay2: "9399b2",
  overlay1: "7f849c",
  overlay0: "6c7086",
  surface2: "585b70",
  surface1: "45475a",
  surface0: "313244",
  base: "1e1e2e",
  mantle: "181825",
  crust: "11111b"
};
var catppuccinMocha = {
  base: "vs-dark",
  inherit: true,
  rules: [
    // Comments
    { token: "comment", foreground: mocha.overlay1, fontStyle: "italic" },
    { token: "comment.doc", foreground: mocha.overlay2, fontStyle: "italic" },
    // Keys - pink/flamingo for that warm feel
    { token: "key", foreground: mocha.flamingo },
    { token: "string.key", foreground: mocha.flamingo },
    { token: "tag.key", foreground: mocha.mauve },
    // Values - sapphire/blue
    { token: "value", foreground: mocha.sapphire },
    // Tags - mauve (purple)
    { token: "tag", foreground: mocha.mauve },
    // Strings - green
    { token: "string", foreground: mocha.green },
    { token: "string.heredoc", foreground: mocha.green },
    { token: "string.escape", foreground: mocha.peach },
    // Delimiters
    { token: "delimiter.curly", foreground: mocha.yellow },
    { token: "delimiter.parenthesis", foreground: mocha.pink },
    { token: "delimiter.comma", foreground: mocha.overlay2 },
    // Invalid
    { token: "invalid", foreground: mocha.red },
    // Additional token types for embedded languages
    { token: "keyword", foreground: mocha.mauve },
    { token: "keyword.sql", foreground: mocha.mauve },
    { token: "operator", foreground: mocha.sky },
    { token: "operator.sql", foreground: mocha.sky },
    { token: "number", foreground: mocha.peach },
    { token: "number.json", foreground: mocha.peach },
    { token: "identifier", foreground: mocha.text },
    { token: "type", foreground: mocha.yellow },
    { token: "type.identifier.json", foreground: mocha.blue },
    { token: "predefined", foreground: mocha.blue },
    { token: "predefined.sql", foreground: mocha.blue },
    // JSON specific
    { token: "string.key.json", foreground: mocha.flamingo },
    { token: "string.value.json", foreground: mocha.green },
    { token: "keyword.json", foreground: mocha.mauve },
    { token: "delimiter.bracket.json", foreground: mocha.overlay2 },
    { token: "delimiter.array.json", foreground: mocha.pink },
    { token: "delimiter.colon.json", foreground: mocha.overlay2 },
    { token: "delimiter.comma.json", foreground: mocha.overlay2 }
  ],
  colors: {
    "editor.background": "#" + mocha.base,
    "editor.foreground": "#" + mocha.text,
    "editor.lineHighlightBackground": "#" + mocha.surface0,
    "editorCursor.foreground": "#" + mocha.rosewater,
    "editor.selectionBackground": "#" + mocha.surface2 + "80",
    "editorLineNumber.foreground": "#" + mocha.surface2,
    "editorLineNumber.activeForeground": "#" + mocha.lavender,
    "editorIndentGuide.background": "#" + mocha.surface1,
    "editorIndentGuide.activeBackground": "#" + mocha.surface2,
    "editorBracketMatch.background": "#" + mocha.surface2 + "80",
    "editorBracketMatch.border": "#" + mocha.mauve,
    "editor.findMatchBackground": "#" + mocha.peach + "40",
    "editor.findMatchHighlightBackground": "#" + mocha.yellow + "30",
    "editorWidget.background": "#" + mocha.mantle,
    "editorWidget.border": "#" + mocha.surface1,
    "input.background": "#" + mocha.surface0,
    "input.border": "#" + mocha.surface1,
    "input.foreground": "#" + mocha.text,
    "scrollbarSlider.background": "#" + mocha.surface1 + "80",
    "scrollbarSlider.hoverBackground": "#" + mocha.surface2 + "80",
    "scrollbarSlider.activeBackground": "#" + mocha.surface2
  }
};

// src/index.ts
var styxLanguageConfig = {
  comments: { lineComment: "//" },
  brackets: [
    ["{", "}"],
    ["(", ")"]
  ],
  autoClosingPairs: [
    { open: "{", close: "}" },
    { open: "(", close: ")" },
    { open: '"', close: '"' }
  ],
  surroundingPairs: [
    { open: "{", close: "}" },
    { open: "(", close: ")" },
    { open: '"', close: '"' }
  ]
};
function registerStyxLanguage(monacoInstance, embeddedLanguages, options = {}) {
  const { defineTheme = true, registerEmbeddedLanguages = true } = options;
  if (registerEmbeddedLanguages && embeddedLanguages) {
    for (const { id, def } of embeddedLanguages) {
      const existing = monacoInstance.languages.getLanguages().find((l) => l.id === id);
      if (!existing) {
        monacoInstance.languages.register({ id });
      }
      monacoInstance.languages.setMonarchTokensProvider(id, def.language);
      monacoInstance.languages.setLanguageConfiguration(id, def.conf);
    }
  }
  monacoInstance.languages.register({ id: "styx" });
  monacoInstance.languages.setTokensProvider("styx", new StyxTokensProvider(monacoInstance.editor));
  monacoInstance.languages.setLanguageConfiguration("styx", styxLanguageConfig);
  if (defineTheme) {
    monacoInstance.editor.defineTheme("catppuccin-mocha", catppuccinMocha);
  }
}
// Annotate the CommonJS export names for ESM import in node:
0 && (module.exports = {
  StyxTokensProvider,
  catppuccinMocha,
  mocha,
  registerStyxLanguage,
  styxLanguageConfig
});
