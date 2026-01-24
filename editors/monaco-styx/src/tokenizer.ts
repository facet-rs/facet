import type * as monaco from 'monaco-editor';

// Styx tokenizer state
type ContextType = 'object' | 'sequence';
type EntryPhase = 'key' | 'value';

interface HeredocState {
  delimiter: string;
  language: string | null;
  indentation: string | null; // captured when we see closing delimiter
}

interface StyxState extends monaco.languages.IState {
  // Stack of contexts: each entry is either 'object' or 'sequence'
  contextStack: ContextType[];
  // Current entry phase (only meaningful in object context)
  entryPhase: EntryPhase;
  // Heredoc state (when inside heredoc content)
  heredoc: HeredocState | null;
  // Raw string hash count (when inside raw string)
  rawStringHashes: number | null;
  // String state (when inside quoted string)
  inString: boolean;
  // Is this a key string or value string?
  stringIsKey: boolean;
}

function createInitialState(): StyxState {
  return {
    contextStack: ['object'], // document is implicit root object
    entryPhase: 'key',
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
        equals: this.equals,
      };
    },
    equals(other: monaco.languages.IState): boolean {
      const o = other as StyxState;
      return (
        this.contextStack.length === o.contextStack.length &&
        this.contextStack.every((v, i) => v === o.contextStack[i]) &&
        this.entryPhase === o.entryPhase &&
        JSON.stringify(this.heredoc) === JSON.stringify(o.heredoc) &&
        this.rawStringHashes === o.rawStringHashes &&
        this.inString === o.inString &&
        this.stringIsKey === o.stringIsKey
      );
    },
  };
}

function currentContext(state: StyxState): ContextType {
  return state.contextStack[state.contextStack.length - 1] || 'object';
}

function isInObjectContext(state: StyxState): boolean {
  return currentContext(state) === 'object';
}

// Token types used by Monaco
const TOKEN = {
  WHITE: 'white',
  COMMENT: 'comment',
  COMMENT_DOC: 'comment.doc',
  KEY: 'key',
  STRING_KEY: 'string.key',
  TAG_KEY: 'tag.key',
  VALUE: 'value',
  TAG: 'tag',
  STRING: 'string',
  STRING_HEREDOC: 'string.heredoc',
  STRING_ESCAPE: 'string.escape',
  DELIMITER_CURLY: 'delimiter.curly',
  DELIMITER_PAREN: 'delimiter.parenthesis',
  DELIMITER_COMMA: 'delimiter.comma',
  INVALID: 'invalid',
};

interface Token {
  startIndex: number;
  scopes: string;
}

// Regex patterns
const WHITESPACE = /^[ \t]+/;
const DOC_COMMENT = /^\/\/\/.*/;
const LINE_COMMENT = /^\/\/.*/;
const TAG_IDENT = /^@[A-Za-z_][A-Za-z0-9_-]*/;
const UNIT = /^@(?![A-Za-z_])/;
const HEREDOC_START = /^<<([A-Z][A-Z0-9_]*)(?:,([a-z][a-z0-9_.-]*))?/;
const RAW_STRING_START = /^r(#+)"/;
// Bare scalar: first char not in forbidden set, subsequent chars allow @ and =, but not >
const BARE_FIRST_CHAR = /^[^\s{}()\,\"=@>\r\n]/;
const BARE_CONT_CHAR = /^[^\s{}()\,\">\r\n]/;

/**
 * Monaco tokens provider for Styx language.
 * Handles context-aware tokenization including heredocs and embedded language injection.
 */
export class StyxTokensProvider implements monaco.languages.TokensProvider {
  private monacoEditor: typeof monaco.editor | undefined;

  /**
   * @param monacoEditor Optional monaco.editor reference for embedded language tokenization.
   *                     If not provided, heredocs will be styled as plain heredoc strings.
   */
  constructor(monacoEditor?: typeof monaco.editor) {
    this.monacoEditor = monacoEditor;
  }

  getInitialState(): monaco.languages.IState {
    return createInitialState();
  }

  tokenize(line: string, inputState: monaco.languages.IState): monaco.languages.ILineTokens {
    const state = (inputState as StyxState).clone() as StyxState;
    const tokens: Token[] = [];
    let pos = 0;

    const addToken = (start: number, type: string) => {
      tokens.push({ startIndex: start, scopes: type });
    };

    // Helper to determine token type based on context and phase
    const atomType = (isTag: boolean): string => {
      if (!isInObjectContext(state)) {
        // In sequence context, everything is a value
        return isTag ? TOKEN.TAG : TOKEN.VALUE;
      }
      // In object context
      if (state.entryPhase === 'key') {
        return isTag ? TOKEN.TAG_KEY : TOKEN.KEY;
      }
      return isTag ? TOKEN.TAG : TOKEN.VALUE;
    };

    const stringType = (): string => {
      if (!isInObjectContext(state)) {
        return TOKEN.STRING;
      }
      return state.entryPhase === 'key' ? TOKEN.STRING_KEY : TOKEN.STRING;
    };

    // After consuming an atom, update entry phase
    const afterAtom = () => {
      if (isInObjectContext(state)) {
        if (state.entryPhase === 'key') {
          state.entryPhase = 'value';
        } else {
          // After value, entry is complete, next atom is a key
          state.entryPhase = 'key';
        }
      }
      // In sequence context, phase doesn't change
    };

    // Handle heredoc content
    if (state.heredoc) {
      const delim = state.heredoc.delimiter;
      // Check for closing delimiter (possibly indented)
      const closeMatch = line.match(new RegExp(`^(\\s*)(${delim})\\s*$`));
      if (closeMatch) {
        // Closing delimiter line
        addToken(0, TOKEN.STRING_HEREDOC);
        state.heredoc = null;
        afterAtom();
        return { tokens, endState: state };
      }

      // Content line - check for language injection
      const lang = state.heredoc.language;
      if (lang && this.monacoEditor) {
        // Try to use Monaco's built-in tokenizer for the embedded language
        try {
          const embeddedTokens = this.monacoEditor.tokenize(line, lang);
          if (embeddedTokens.length > 0 && embeddedTokens[0].length > 0) {
            // Use the embedded language's tokens
            for (const token of embeddedTokens[0]) {
              tokens.push({
                startIndex: token.offset,
                scopes: token.type,
              });
            }
            return { tokens, endState: state };
          }
        } catch {
          // Language not available, fall back to heredoc style
        }
      }

      // Default: style as heredoc string
      addToken(0, TOKEN.STRING_HEREDOC);
      return { tokens, endState: state };
    }

    // Handle continued quoted string
    if (state.inString) {
      const tokenType = state.stringIsKey ? TOKEN.STRING_KEY : TOKEN.STRING;
      while (pos < line.length) {
        const ch = line[pos];
        if (ch === '\\' && pos + 1 < line.length) {
          // Escape sequence
          if (tokens.length === 0 || tokens[tokens.length - 1].startIndex !== pos) {
            addToken(pos, TOKEN.STRING_ESCAPE);
          }
          pos += 2;
          if (pos < line.length) {
            addToken(pos, tokenType);
          }
        } else if (ch === '"') {
          // End of string
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
        // String continues to next line (invalid in Styx, but highlight gracefully)
        if (tokens.length === 0) {
          addToken(0, tokenType);
        }
        return { tokens, endState: state };
      }
    }

    // Handle continued raw string
    if (state.rawStringHashes !== null) {
      const hashes = state.rawStringHashes;
      const tokenType = state.stringIsKey ? TOKEN.STRING_KEY : TOKEN.STRING;
      const closePattern = '"' + '#'.repeat(hashes);

      while (pos < line.length) {
        const idx = line.indexOf(closePattern, pos);
        if (idx >= 0) {
          addToken(pos, tokenType);
          pos = idx + closePattern.length;
          state.rawStringHashes = null;
          afterAtom();
          break;
        } else {
          // No closing on this line
          addToken(pos, tokenType);
          return { tokens, endState: state };
        }
      }
    }

    // Main tokenization loop
    while (pos < line.length) {
      const rest = line.slice(pos);
      let match: RegExpMatchArray | null;

      // Whitespace
      if ((match = rest.match(WHITESPACE))) {
        addToken(pos, TOKEN.WHITE);
        pos += match[0].length;
        continue;
      }

      // Doc comment
      if ((match = rest.match(DOC_COMMENT))) {
        addToken(pos, TOKEN.COMMENT_DOC);
        pos += match[0].length;
        continue;
      }

      // Line comment
      if ((match = rest.match(LINE_COMMENT))) {
        addToken(pos, TOKEN.COMMENT);
        pos += match[0].length;
        continue;
      }

      // Opening brace - starts object context
      if (rest[0] === '{') {
        addToken(pos, TOKEN.DELIMITER_CURLY);
        pos++;
        state.contextStack.push('object');
        state.entryPhase = 'key';
        continue;
      }

      // Closing brace - ends object context
      if (rest[0] === '}') {
        addToken(pos, TOKEN.DELIMITER_CURLY);
        pos++;
        state.contextStack.pop();
        // After closing brace, if we're back in object context, the brace was the value
        // So entry is complete
        if (isInObjectContext(state) && state.entryPhase === 'value') {
          state.entryPhase = 'key';
        }
        continue;
      }

      // Opening paren - starts sequence context
      if (rest[0] === '(') {
        addToken(pos, TOKEN.DELIMITER_PAREN);
        pos++;
        state.contextStack.push('sequence');
        continue;
      }

      // Closing paren - ends sequence context
      if (rest[0] === ')') {
        addToken(pos, TOKEN.DELIMITER_PAREN);
        pos++;
        state.contextStack.pop();
        // After closing paren, if we're back in object context, the sequence was the value
        if (isInObjectContext(state) && state.entryPhase === 'value') {
          state.entryPhase = 'key';
        }
        continue;
      }

      // Comma - entry separator in object, invalid in sequence
      if (rest[0] === ',') {
        addToken(pos, TOKEN.DELIMITER_COMMA);
        pos++;
        // Comma ends the current entry, next atom is a key
        if (isInObjectContext(state)) {
          state.entryPhase = 'key';
        }
        continue;
      }

      // Heredoc
      if ((match = rest.match(HEREDOC_START))) {
        addToken(pos, TOKEN.STRING_HEREDOC);
        pos += match[0].length;
        state.heredoc = {
          delimiter: match[1],
          language: match[2] || null,
          indentation: null,
        };
        // Heredoc continues to the next line
        return { tokens, endState: state };
      }

      // Raw string
      if ((match = rest.match(RAW_STRING_START))) {
        const hashes = match[1].length;
        const isKey = isInObjectContext(state) && state.entryPhase === 'key';
        addToken(pos, isKey ? TOKEN.STRING_KEY : TOKEN.STRING);
        pos += match[0].length;

        // Look for closing
        const closePattern = '"' + '#'.repeat(hashes);
        const closeIdx = line.indexOf(closePattern, pos);
        if (closeIdx >= 0) {
          // Found closing on same line
          pos = closeIdx + closePattern.length;
          afterAtom();
        } else {
          // Continues to next line
          state.rawStringHashes = hashes;
          state.stringIsKey = isKey;
          return { tokens, endState: state };
        }
        continue;
      }

      // Quoted string
      if (rest[0] === '"') {
        const isKey = isInObjectContext(state) && state.entryPhase === 'key';
        const tokenType = isKey ? TOKEN.STRING_KEY : TOKEN.STRING;
        addToken(pos, tokenType);
        pos++;

        // Parse the string
        while (pos < line.length) {
          const ch = line[pos];
          if (ch === '\\' && pos + 1 < line.length) {
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
          // Unclosed string, continues to next line
          state.inString = true;
          state.stringIsKey = isKey;
        }
        continue;
      }

      // Unit (@)
      if ((match = rest.match(UNIT))) {
        addToken(pos, atomType(true));
        pos += match[0].length;
        afterAtom();
        continue;
      }

      // Tag with identifier
      if ((match = rest.match(TAG_IDENT))) {
        addToken(pos, atomType(true));
        pos += match[0].length;

        // Check for immediate payload (no whitespace)
        const afterTag = line.slice(pos);
        if (afterTag[0] === '{' || afterTag[0] === '(') {
          // Payload will be handled as separate atom by the braces
          // But actually for tags like @tag{...}, the whole thing is one atom
          // The brace handling will take care of context, but we should NOT call afterAtom yet
          // Actually, let's reconsider: @tag{...} is ONE atom (tagged object)
          // So after the closing }, that's when the atom ends
          // For now, let's NOT call afterAtom here, let the closing brace handle it
          // But wait, that means the brace opens a context and after closing,
          // we'd still be at 'value' phase which is wrong
          //
          // Let me re-read the spec... "A tag MAY be immediately followed (no whitespace) by a payload"
          // So @tag{...} is a tagged object, which is ONE value atom
          //
          // Actually the way our state machine works:
          // 1. See @tag - it's a tag, phase becomes 'value' (if was 'key')
          // 2. See { - opens object context, phase becomes 'key'
          // 3. Inside object, process entries
          // 4. See } - pops object context
          // 5. Back in parent context, we need to know the entire @tag{...} was ONE atom
          //
          // This is tricky. For immediate payloads, we should NOT advance phase after the tag.
          // We should wait for the payload to complete.
          //
          // For now, let's do it simply: tag without immediate payload = afterAtom()
          // tag with immediate payload = don't afterAtom(), let the closing delimiter do it
          continue; // Don't call afterAtom
        } else if (afterTag[0] === '"' || afterTag.match(/^r#+"/)) {
          // Tag with string payload - let the string parsing handle it
          // Don't call afterAtom yet
          continue;
        } else if (afterTag.match(/^<<[A-Z]/)) {
          // Tag with heredoc payload
          continue;
        }

        // Tag with no immediate payload (tagged unit or standalone tag)
        afterAtom();
        continue;
      }

      // Bare scalar
      if (rest.match(BARE_FIRST_CHAR)) {
        const startPos = pos;
        pos++;
        // Continue consuming
        while (pos < line.length && line.slice(pos).match(BARE_CONT_CHAR)) {
          pos++;
        }

        // Check for attribute syntax (key>value)
        addToken(startPos, atomType(false));
        afterAtom();
        continue;
      }

      // Unknown character - mark as invalid
      addToken(pos, TOKEN.INVALID);
      pos++;
    }

    // End of line - in object context, newline ends the entry
    if (isInObjectContext(state)) {
      state.entryPhase = 'key';
    }

    return { tokens, endState: state };
  }
}
