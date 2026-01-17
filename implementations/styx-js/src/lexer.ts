import { Span, ParseError } from "./types.js";

export type TokenType =
  | "scalar"
  | "quoted"
  | "raw"
  | "heredoc"
  | "lbrace"
  | "rbrace"
  | "lparen"
  | "rparen"
  | "comma"
  | "at"
  | "tag"
  | "gt"
  | "newline"
  | "eof";

export interface Token {
  type: TokenType;
  text: string;
  span: Span;
  /** True if there was whitespace before this token */
  hadWhitespaceBefore: boolean;
  /** True if there was a newline before this token */
  hadNewlineBefore: boolean;
}

const SPECIAL_CHARS = new Set(["{", "}", "(", ")", ",", '"', ">", " ", "\t", "\n", "\r"]);

export class Lexer {
  private pos = 0; // character position
  private bytePos = 0; // byte position for spans
  private line = 1;
  private col = 1;

  constructor(private source: string) {}

  private peek(offset = 0): string {
    return this.source[this.pos + offset] ?? "";
  }

  private advance(): string {
    if (this.pos >= this.source.length) return "";

    // Check for surrogate pair (emoji, etc.)
    const code = this.source.charCodeAt(this.pos);
    let ch: string;
    if (code >= 0xd800 && code <= 0xdbff && this.pos + 1 < this.source.length) {
      // High surrogate - consume both code units
      ch = this.source.slice(this.pos, this.pos + 2);
      this.pos += 2;
    } else {
      ch = this.source[this.pos++];
    }

    // Calculate UTF-8 byte length of the character
    this.bytePos += this.utf8ByteLength(ch);
    if (ch === "\n") {
      this.line++;
      this.col = 1;
    } else {
      this.col++;
    }
    return ch;
  }

  private utf8ByteLength(ch: string): number {
    // Use TextEncoder to get accurate UTF-8 byte length
    // This handles surrogate pairs and all Unicode correctly
    return new TextEncoder().encode(ch).length;
  }

  /** Get current byte position for span start */
  private get byteStart(): number {
    return this.bytePos;
  }

  private skipWhitespaceAndComments(): { hadWhitespace: boolean; hadNewline: boolean } {
    let hadWhitespace = false;
    let hadNewline = false;
    while (this.pos < this.source.length) {
      const ch = this.peek();
      if (ch === " " || ch === "\t" || ch === "\r") {
        hadWhitespace = true;
        this.advance();
      } else if (ch === "\n") {
        hadWhitespace = true;
        hadNewline = true;
        this.advance();
      } else if (ch === "/" && this.peek(1) === "/") {
        // Line comment - skip to end of line
        hadWhitespace = true;
        while (this.pos < this.source.length && this.peek() !== "\n") {
          this.advance();
        }
      } else {
        break;
      }
    }
    return { hadWhitespace, hadNewline };
  }

  nextToken(): Token {
    const { hadWhitespace, hadNewline } = this.skipWhitespaceAndComments();

    if (this.pos >= this.source.length) {
      return {
        type: "eof",
        text: "",
        span: { start: this.bytePos, end: this.bytePos },
        hadWhitespaceBefore: hadWhitespace,
        hadNewlineBefore: hadNewline,
      };
    }

    const start = this.bytePos;
    const ch = this.peek();

    // Single-character tokens
    if (ch === "{") {
      this.advance();
      return {
        type: "lbrace",
        text: "{",
        span: { start, end: this.bytePos },
        hadWhitespaceBefore: hadWhitespace,
        hadNewlineBefore: hadNewline,
      };
    }
    if (ch === "}") {
      this.advance();
      return {
        type: "rbrace",
        text: "}",
        span: { start, end: this.bytePos },
        hadWhitespaceBefore: hadWhitespace,
        hadNewlineBefore: hadNewline,
      };
    }
    if (ch === "(") {
      this.advance();
      return {
        type: "lparen",
        text: "(",
        span: { start, end: this.bytePos },
        hadWhitespaceBefore: hadWhitespace,
        hadNewlineBefore: hadNewline,
      };
    }
    if (ch === ")") {
      this.advance();
      return {
        type: "rparen",
        text: ")",
        span: { start, end: this.bytePos },
        hadWhitespaceBefore: hadWhitespace,
        hadNewlineBefore: hadNewline,
      };
    }
    if (ch === ",") {
      this.advance();
      return {
        type: "comma",
        text: ",",
        span: { start, end: this.bytePos },
        hadWhitespaceBefore: hadWhitespace,
        hadNewlineBefore: hadNewline,
      };
    }
    if (ch === ">") {
      this.advance();
      return {
        type: "gt",
        text: ">",
        span: { start, end: this.bytePos },
        hadWhitespaceBefore: hadWhitespace,
        hadNewlineBefore: hadNewline,
      };
    }

    // @ - either unit or tag
    if (ch === "@") {
      this.advance();
      // Check if it's a tag name
      if (this.isTagStart(this.peek())) {
        const nameStart = this.pos;
        while (this.isTagChar(this.peek())) {
          this.advance();
        }
        const name = this.source.slice(nameStart, this.pos);
        return {
          type: "tag",
          text: name,
          span: { start, end: this.bytePos },
          hadWhitespaceBefore: hadWhitespace,
          hadNewlineBefore: hadNewline,
        };
      }
      return {
        type: "at",
        text: "@",
        span: { start, end: this.bytePos },
        hadWhitespaceBefore: hadWhitespace,
        hadNewlineBefore: hadNewline,
      };
    }

    // Quoted string
    if (ch === '"') {
      return this.readQuotedString(start, hadWhitespace, hadNewline);
    }

    // Raw string
    if (ch === "r" && (this.peek(1) === '"' || this.peek(1) === "#")) {
      return this.readRawString(start, hadWhitespace, hadNewline);
    }

    // Heredoc - only if << is followed by uppercase letter
    if (ch === "<" && this.peek(1) === "<") {
      const afterLtLt = this.peek(2);
      if (afterLtLt >= "A" && afterLtLt <= "Z") {
        return this.readHeredoc(start, hadWhitespace, hadNewline);
      }
      // << not followed by uppercase - skip << and any following digits
      this.advance(); // <
      this.advance(); // <
      while (this.peek() >= "0" && this.peek() <= "9") {
        this.advance();
      }
      return this.nextToken();
    }

    // Bare scalar
    return this.readBareScalar(start, hadWhitespace, hadNewline);
  }

  private isTagStart(ch: string): boolean {
    return /[A-Za-z_]/.test(ch);
  }

  private isTagChar(ch: string): boolean {
    return /[A-Za-z0-9_\-]/.test(ch);
  }

  private readQuotedString(start: number, hadWhitespace: boolean, hadNewline: boolean): Token {
    this.advance(); // opening "
    let text = "";

    while (this.pos < this.source.length) {
      const ch = this.peek();
      if (ch === '"') {
        this.advance();
        return {
          type: "quoted",
          text,
          span: { start, end: this.bytePos },
          hadWhitespaceBefore: hadWhitespace,
          hadNewlineBefore: hadNewline,
        };
      }
      if (ch === "\\") {
        const escapeStart = this.bytePos;
        this.advance();
        const escaped = this.advance();
        switch (escaped) {
          case "n":
            text += "\n";
            break;
          case "r":
            text += "\r";
            break;
          case "t":
            text += "\t";
            break;

          case "\\":
            text += "\\";
            break;
          case '"':
            text += '"';
            break;
          case "u":
            text += this.readUnicodeEscape();
            break;
          default:
            // Unknown escape - this is a parse error
            throw new ParseError(`invalid escape sequence: \\${escaped}`, {
              start: escapeStart,
              end: this.bytePos,
            });
        }
      } else if (ch === "\n") {
        // Unclosed string at newline - recover by including the raw content
        // Include the opening quote in text to match Rust behavior
        this.advance(); // consume newline
        return {
          type: "quoted",
          text: '"' + text + "\n",
          span: { start, end: this.bytePos },
          hadWhitespaceBefore: hadWhitespace,
          hadNewlineBefore: hadNewline,
        };
      } else if (ch === "\r") {
        // Handle \r\n
        this.advance();
        if (this.peek() === "\n") {
          this.advance();
        }
        return {
          type: "quoted",
          text: '"' + text + "\n",
          span: { start, end: this.bytePos },
          hadWhitespaceBefore: hadWhitespace,
          hadNewlineBefore: hadNewline,
        };
      } else {
        text += this.advance();
      }
    }

    // EOF - recover by including the raw content
    return {
      type: "quoted",
      text: '"' + text,
      span: { start, end: this.bytePos },
      hadWhitespaceBefore: hadWhitespace,
      hadNewlineBefore: hadNewline,
    };
  }

  private readUnicodeEscape(): string {
    if (this.peek() === "{") {
      this.advance(); // {
      let hex = "";
      while (this.peek() !== "}" && this.pos < this.source.length) {
        hex += this.advance();
      }
      this.advance(); // }
      const codePoint = parseInt(hex, 16);
      return String.fromCodePoint(codePoint);
    } else {
      // \uXXXX format
      let hex = "";
      for (let i = 0; i < 4; i++) {
        hex += this.advance();
      }
      const codePoint = parseInt(hex, 16);
      return String.fromCodePoint(codePoint);
    }
  }

  private readRawString(start: number, hadWhitespace: boolean, hadNewline: boolean): Token {
    this.advance(); // r
    let hashes = 0;
    while (this.peek() === "#") {
      this.advance();
      hashes++;
    }
    this.advance(); // opening "

    let text = "";
    const closePattern = '"' + "#".repeat(hashes);

    while (this.pos < this.source.length) {
      if (this.source.slice(this.pos, this.pos + closePattern.length) === closePattern) {
        // Advance through the close pattern
        for (let i = 0; i < closePattern.length; i++) {
          this.advance();
        }
        return {
          type: "raw",
          text,
          span: { start, end: this.bytePos },
          hadWhitespaceBefore: hadWhitespace,
          hadNewlineBefore: hadNewline,
        };
      }
      text += this.advance();
    }

    throw new ParseError("unclosed raw string", { start, end: this.bytePos });
  }

  private readHeredoc(start: number, hadWhitespace: boolean, hadNewline: boolean): Token {
    this.advance(); // <
    this.advance(); // <

    // Read delimiter (and optional language hint)
    let delimiter = "";
    while (this.pos < this.source.length && this.peek() !== "\n") {
      delimiter += this.advance();
    }
    if (this.pos < this.source.length) {
      this.advance(); // newline
    }

    // Track opener span (<<DELIM\n) for error reporting
    const openerEnd = this.bytePos;

    // Read content until delimiter on its own line
    let text = "";
    const bareDelimiter = delimiter.split(",")[0];

    while (this.pos < this.source.length) {
      // Read a line
      let line = "";
      while (this.pos < this.source.length && this.peek() !== "\n") {
        line += this.advance();
      }

      // Check for closing delimiter
      if (line === bareDelimiter) {
        // Include language hint in text if present
        if (delimiter.includes(",")) {
          text = delimiter.slice(bareDelimiter.length) + "\n" + text;
        }
        return {
          type: "heredoc",
          text,
          span: { start, end: this.bytePos },
          hadWhitespaceBefore: hadWhitespace,
          hadNewlineBefore: hadNewline,
        };
      }

      // Add line to content
      text += line;
      if (this.pos < this.source.length && this.peek() === "\n") {
        this.advance();
        text += "\n";
      }
    }

    // Heredoc without closing delimiter - use opener span only
    if (delimiter.includes(",")) {
      text = delimiter.slice(delimiter.split(",")[0].length) + "\n" + text;
    }
    return {
      type: "heredoc",
      text,
      span: { start, end: openerEnd },
      hadWhitespaceBefore: hadWhitespace,
      hadNewlineBefore: hadNewline,
    };
  }

  private readBareScalar(start: number, hadWhitespace: boolean, hadNewline: boolean): Token {
    let text = "";
    while (this.pos < this.source.length) {
      const ch = this.peek();
      if (SPECIAL_CHARS.has(ch)) {
        break;
      }
      text += this.advance();
    }
    return {
      type: "scalar",
      text,
      span: { start, end: this.bytePos },
      hadWhitespaceBefore: hadWhitespace,
      hadNewlineBefore: hadNewline,
    };
  }
}
