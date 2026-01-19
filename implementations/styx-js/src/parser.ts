import { Lexer, Token, TokenType } from "./lexer.js";
import {
  Value,
  Scalar,
  Sequence,
  StyxObject,
  Entry,
  Document,
  Span,
  ParseError,
  Separator,
  ScalarKind,
  PathState,
  PathValueKind,
} from "./types.js";

export class Parser {
  private lexer: Lexer;
  private current: Token;
  private peeked: Token | null = null;

  constructor(source: string) {
    this.lexer = new Lexer(source);
    this.current = this.lexer.nextToken();
  }

  private advance(): Token {
    const prev = this.current;
    if (this.peeked) {
      this.current = this.peeked;
      this.peeked = null;
    } else {
      this.current = this.lexer.nextToken();
    }
    return prev;
  }

  private peek(): Token {
    if (!this.peeked) {
      this.peeked = this.lexer.nextToken();
    }
    return this.peeked;
  }

  private check(...types: TokenType[]): boolean {
    return types.includes(this.current.type);
  }

  private expect(type: TokenType): Token {
    if (this.current.type !== type) {
      throw new ParseError(`expected ${type}, got ${this.current.type}`, this.current.span);
    }
    return this.advance();
  }

  parse(): Document {
    const entries: Entry[] = [];
    const start = this.current.span.start;
    const pathState = new PathState();

    while (!this.check("eof")) {
      const entry = this.parseEntryWithPathCheck(pathState);
      if (entry) {
        entries.push(entry);
      }
    }

    return {
      entries,
      span: { start, end: this.current.span.end },
    };
  }

  private parseEntryWithPathCheck(pathState: PathState): Entry | null {
    while (this.check("comma")) {
      this.advance();
    }

    // Trailing > without a value is a parse error
    if (this.check("gt")) {
      throw new ParseError("expected a value", this.current.span);
    }

    if (this.check("eof", "rbrace")) {
      return null;
    }

    // Parse the key
    const key = this.parseValue();

    // Special case: object in key position gets implicit unit key
    if (key.payload?.type === "object") {
      if (!this.current.hadNewlineBefore && !this.check("eof", "rbrace", "comma")) {
        this.parseValue(); // Drop trailing value
      }
      const unitKey: Value = { span: { start: -1, end: -1 } };
      return { key: unitKey, value: key };
    }

    // Check for dotted path in bare scalar key
    if (key.payload?.type === "scalar" && key.payload.kind === "bare") {
      const text = key.payload.text;
      if (text.includes(".")) {
        return this.expandDottedPathWithState(text, key.span, pathState);
      }
    }

    // Get key text for path state
    const keyText = this.getKeyText(key);

    // Validate key
    this.validateKey(key);

    // Check for implicit unit (key followed by newline, EOF, or closing brace)
    if (this.current.hadNewlineBefore || this.check("eof", "rbrace")) {
      if (keyText !== null) {
        pathState.checkAndUpdate([keyText], key.span, "terminal");
      }
      const unitValue: Value = { span: key.span };
      return { key, value: unitValue };
    }

    // Parse the value
    const value = this.parseValue();

    // Determine kind from actual value
    if (keyText !== null) {
      const kind: PathValueKind = value.payload?.type === "object" ? "object" : "terminal";
      pathState.checkAndUpdate([keyText], key.span, kind);
    }

    return { key, value };
  }

  private expandDottedPathWithState(pathText: string, span: Span, pathState: PathState): Entry {
    const segments = pathText.split(".");

    if (segments.some((s) => s === "")) {
      throw new ParseError("invalid key", span);
    }

    // Calculate byte offsets for each segment
    const segmentSpans: Span[] = [];
    let offset = span.start;
    for (let i = 0; i < segments.length; i++) {
      const segmentBytes = new TextEncoder().encode(segments[i]).length;
      segmentSpans.push({ start: offset, end: offset + segmentBytes });
      offset += segmentBytes + 1; // +1 for the dot
    }

    const value = this.parseValue();

    // Determine value kind
    const kind: PathValueKind = value.payload?.type === "object" ? "object" : "terminal";

    // Check and update path state - use full path span for error messages
    pathState.checkAndUpdate(segments, span, kind);

    // Build nested structure from inside out
    // Object spans start at the PREVIOUS segment's position (i-1)
    const lastKeyEnd = segmentSpans[segments.length - 1].end;
    let result: Value = value;
    for (let i = segments.length - 1; i >= 1; i--) {
      const segSpan = segmentSpans[i];
      const segmentKey: Value = {
        payload: {
          type: "scalar",
          text: segments[i],
          kind: "bare",
          span: segSpan,
        },
        span: segSpan,
      };
      // Object span starts at the previous segment's position
      const objStart = segmentSpans[i - 1].start;
      const objSpan = { start: objStart, end: lastKeyEnd };
      result = {
        payload: {
          type: "object",
          entries: [{ key: segmentKey, value: result }],
          separator: "newline",
          span: objSpan,
        },
        span: objSpan,
      };
    }

    const firstSpan = segmentSpans[0];
    const outerKey: Value = {
      payload: {
        type: "scalar",
        text: segments[0],
        kind: "bare",
        span: firstSpan,
      },
      span: firstSpan,
    };

    return { key: outerKey, value: result };
  }

  private parseEntryWithDupCheck(seenKeys: Map<string, Span>): Entry | null {
    while (this.check("comma")) {
      this.advance();
    }

    // Trailing > without a value is a parse error
    if (this.check("gt")) {
      throw new ParseError("expected a value", this.current.span);
    }

    if (this.check("eof", "rbrace")) {
      return null;
    }

    // Parse the key
    const key = this.parseValue();

    // Special case: object in key position gets implicit unit key
    // The parsed "value" is dropped (matches Rust styx_tree behavior)
    if (key.payload?.type === "object") {
      // Skip any trailing value on the same line
      if (!this.current.hadNewlineBefore && !this.check("eof", "rbrace", "comma")) {
        this.parseValue(); // Drop it
      }
      const unitKey: Value = { span: { start: -1, end: -1 } };
      return { key: unitKey, value: key };
    }

    // Check for dotted path in bare scalar key
    if (key.payload?.type === "scalar" && key.payload.kind === "bare") {
      const text = key.payload.text;
      if (text.includes(".")) {
        return this.expandDottedPath(text, key.span, seenKeys);
      }
    }

    // Check for duplicate key
    const keyText = this.getKeyText(key);
    if (keyText !== null) {
      const existing = seenKeys.get(keyText);
      if (existing) {
        throw new ParseError(`duplicate key`, key.span);
      }
      seenKeys.set(keyText, key.span);
    }

    // Validate key
    this.validateKey(key);

    // Check for implicit unit (key followed by newline, EOF, or closing brace)
    if (this.current.hadNewlineBefore || this.check("eof", "rbrace")) {
      // Implicit unit - no value provided
      const unitValue: Value = { span: key.span };
      return { key, value: unitValue };
    }

    // Parse the value
    const value = this.parseValue();

    return { key, value };
  }

  private getKeyText(key: Value): string | null {
    if (key.payload?.type === "scalar") {
      return key.payload.text;
    }
    if (key.tag && !key.payload) {
      return `@${key.tag.name}`;
    }
    return null;
  }

  private validateKey(key: Value): void {
    if (key.payload) {
      // Sequences cannot be used as keys
      if (key.payload.type === "sequence") {
        throw new ParseError(`invalid key`, key.span);
      }
      // Heredocs cannot be used as keys
      if (key.payload.type === "scalar" && key.payload.kind === "heredoc") {
        throw new ParseError(`invalid key`, key.span);
      }
    }
  }

  private expandDottedPath(pathText: string, span: Span, seenKeys: Map<string, Span>): Entry {
    const segments = pathText.split(".");

    if (segments.some((s) => s === "")) {
      throw new ParseError(`invalid key`, span);
    }

    const firstSegment = segments[0];
    const existing = seenKeys.get(firstSegment);
    if (existing) {
      throw new ParseError(`duplicate key`, span);
    }
    seenKeys.set(firstSegment, span);

    // Calculate byte offsets for each segment
    const segmentSpans: Span[] = [];
    let offset = span.start;
    for (let i = 0; i < segments.length; i++) {
      const segmentBytes = new TextEncoder().encode(segments[i]).length;
      segmentSpans.push({ start: offset, end: offset + segmentBytes });
      offset += segmentBytes + 1; // +1 for the dot
    }

    const value = this.parseValue();

    // Build nested structure from inside out
    let result: Value = value;
    for (let i = segments.length - 1; i >= 1; i--) {
      const segSpan = segmentSpans[i];
      const segmentKey: Value = {
        payload: {
          type: "scalar",
          text: segments[i],
          kind: "bare",
          span: segSpan,
        },
        span: segSpan,
      };
      result = {
        payload: {
          type: "object",
          entries: [{ key: segmentKey, value: result }],
          separator: "newline",
          span,
        },
        span,
      };
    }

    const firstSpan = segmentSpans[0];
    const outerKey: Value = {
      payload: {
        type: "scalar",
        text: firstSegment,
        kind: "bare",
        span: firstSpan,
      },
      span: firstSpan,
    };

    return { key: outerKey, value: result };
  }

  private parseAttributeValue(): Value {
    if (this.check("lbrace")) {
      const obj = this.parseObject();
      return { payload: obj, span: obj.span };
    }
    if (this.check("lparen")) {
      const seq = this.parseSequence();
      return { payload: seq, span: seq.span };
    }
    if (this.check("tag")) {
      return this.parseTagValue();
    }
    if (this.check("at")) {
      const atToken = this.advance();
      return { span: atToken.span };
    }
    const scalar = this.parseScalar();
    return { payload: scalar, span: scalar.span };
  }

  private parseTagValue(): Value {
    const start = this.current.span.start;
    const tagToken = this.advance();
    const tag = {
      name: tagToken.text,
      span: tagToken.span,
    };

    if (!this.current.hadWhitespaceBefore) {
      // Check for invalid tag continuation (e.g., @org/package where / is not a valid tag char)
      if (this.check("scalar")) {
        throw new ParseError("invalid tag name", { start: start + 1, end: this.current.span.end });
      }
      if (this.check("lbrace")) {
        const obj = this.parseObject();
        return { tag, payload: obj, span: obj.span };
      }
      if (this.check("lparen")) {
        const seq = this.parseSequence();
        return { tag, payload: seq, span: seq.span };
      }
      if (this.check("quoted", "raw", "heredoc")) {
        const scalar = this.parseScalar();
        return { tag, payload: scalar, span: scalar.span };
      }
      if (this.check("at")) {
        const atToken = this.advance();
        // For explicit unit (@tag@), the span is just the trailing @
        return { tag, span: atToken.span };
      }
    }

    return { tag, span: { start, end: tagToken.span.end } };
  }

  private parseValue(): Value {
    if (this.check("at")) {
      const atToken = this.advance();
      // Check for invalid tag name: @ followed immediately by non-tag-start character
      if (
        !this.current.hadWhitespaceBefore &&
        !this.check("eof", "rbrace", "rparen", "comma", "lbrace", "lparen")
      ) {
        // This looks like @123 or @-foo - invalid tag name
        // Error span starts after the @, just covering the invalid name
        throw new ParseError(`invalid tag name`, this.current.span);
      }
      return { span: { start: atToken.span.start, end: atToken.span.end } };
    }

    if (this.check("tag")) {
      return this.parseTagValue();
    }

    if (this.check("lbrace")) {
      const obj = this.parseObject();
      return { payload: obj, span: obj.span };
    }

    if (this.check("lparen")) {
      const seq = this.parseSequence();
      return { payload: seq, span: seq.span };
    }

    // Scalar - check for attributes (key>value)
    if (this.check("scalar")) {
      const scalarToken = this.advance();
      const nextToken = this.current;

      if (nextToken.type === "gt" && !nextToken.hadWhitespaceBefore) {
        // Attribute syntax: key>value
        this.advance(); // consume >
        const afterGT = this.current;
        if (afterGT.hadNewlineBefore || this.check("eof", "rbrace", "rparen")) {
          // Trailing > without a value is a parse error
          throw new ParseError("expected a value", nextToken.span);
        }
        // Parse as attributes (we already consumed >)
        return this.parseAttributesAfterGT(scalarToken);
      }

      return {
        payload: {
          type: "scalar",
          text: scalarToken.text,
          kind: "bare",
          span: scalarToken.span,
        },
        span: scalarToken.span,
      };
    }

    const scalar = this.parseScalar();
    return { payload: scalar, span: scalar.span };
  }

  private parseAttributesStartingWith(firstKeyToken: Token): Value {
    const attrs: Entry[] = [];
    const startSpan = firstKeyToken.span;

    this.expect("gt");
    const firstKey: Value = {
      payload: {
        type: "scalar",
        text: firstKeyToken.text,
        kind: "bare",
        span: firstKeyToken.span,
      },
      span: firstKeyToken.span,
    };
    const firstValue = this.parseAttributeValue();
    attrs.push({ key: firstKey, value: firstValue });

    let endSpan = firstValue.span;

    while (this.check("scalar") && !this.current.hadNewlineBefore) {
      const keyToken = this.current;
      const nextToken = this.peek();
      if (nextToken.type !== "gt" || nextToken.hadWhitespaceBefore) {
        break;
      }

      this.advance();
      this.advance();

      const attrKey: Value = {
        payload: {
          type: "scalar",
          text: keyToken.text,
          kind: "bare",
          span: keyToken.span,
        },
        span: keyToken.span,
      };

      const attrValue = this.parseAttributeValue();
      attrs.push({ key: attrKey, value: attrValue });
      endSpan = attrValue.span;
    }

    const obj: StyxObject = {
      type: "object",
      entries: attrs,
      separator: "comma",
      span: { start: startSpan.start, end: endSpan.end },
    };

    return { payload: obj, span: obj.span };
  }

  private parseAttributesAfterGT(firstKeyToken: Token): Value {
    // Same as parseAttributesStartingWith but > was already consumed
    const attrs: Entry[] = [];
    const startSpan = firstKeyToken.span;

    const firstKey: Value = {
      payload: {
        type: "scalar",
        text: firstKeyToken.text,
        kind: "bare",
        span: firstKeyToken.span,
      },
      span: firstKeyToken.span,
    };
    const firstValue = this.parseAttributeValue();
    attrs.push({ key: firstKey, value: firstValue });

    let endSpan = firstValue.span;

    while (this.check("scalar") && !this.current.hadNewlineBefore) {
      const keyToken = this.current;
      const nextToken = this.peek();
      if (nextToken.type !== "gt" || nextToken.hadWhitespaceBefore) {
        break;
      }

      this.advance();
      this.advance();

      const attrKey: Value = {
        payload: {
          type: "scalar",
          text: keyToken.text,
          kind: "bare",
          span: keyToken.span,
        },
        span: keyToken.span,
      };

      const attrValue = this.parseAttributeValue();
      attrs.push({ key: attrKey, value: attrValue });
      endSpan = attrValue.span;
    }

    const obj: StyxObject = {
      type: "object",
      entries: attrs,
      separator: "comma",
      span: { start: startSpan.start, end: endSpan.end },
    };

    return { payload: obj, span: obj.span };
  }

  private parseScalar(): Scalar {
    const token = this.current;
    let kind: ScalarKind;

    switch (token.type) {
      case "scalar":
        kind = "bare";
        break;
      case "quoted":
        kind = "quoted";
        break;
      case "raw":
        kind = "raw";
        break;
      case "heredoc":
        kind = "heredoc";
        break;
      default:
        throw new ParseError(`expected scalar, got ${token.type}`, token.span);
    }

    this.advance();
    return {
      type: "scalar",
      text: token.text,
      kind,
      span: token.span,
    };
  }

  private parseObject(): StyxObject {
    const openBrace = this.expect("lbrace");
    const start = openBrace.span.start;
    const entries: Entry[] = [];
    let separator: Separator | null = null;
    const seenKeys = new Map<string, Span>();

    if (this.current.hadNewlineBefore) {
      separator = "newline";
    }

    while (!this.check("rbrace", "eof")) {
      const entry = this.parseEntryWithDupCheck(seenKeys);
      if (entry) {
        entries.push(entry);
      }

      if (this.check("comma")) {
        if (separator === "newline") {
          throw new ParseError(
            "mixed separators (use either commas or newlines)",
            this.current.span,
          );
        }
        separator = "comma";
        this.advance();
      } else if (!this.check("rbrace", "eof")) {
        if (separator === "comma") {
          throw new ParseError(
            "mixed separators (use either commas or newlines)",
            this.current.span,
          );
        }
        separator = "newline";
      }
    }

    if (separator === null) {
      separator = "comma";
    }

    if (this.check("eof")) {
      throw new ParseError("unclosed object (missing `}`)", openBrace.span);
    }

    const end = this.expect("rbrace").span.end;
    return {
      type: "object",
      entries,
      separator,
      span: { start, end },
    };
  }

  private parseSequence(): Sequence {
    const openParen = this.expect("lparen");
    const start = openParen.span.start;
    const items: Value[] = [];

    while (!this.check("rparen", "eof")) {
      // Check for comma - not allowed in sequences
      if (this.check("comma")) {
        throw new ParseError(
          "unexpected `,` in sequence (sequences are whitespace-separated, not comma-separated)",
          this.current.span,
        );
      }
      items.push(this.parseValue());
    }

    if (this.check("eof")) {
      throw new ParseError("unclosed sequence (missing `)`)", openParen.span);
    }

    const end = this.expect("rparen").span.end;
    return {
      type: "sequence",
      items,
      span: { start, end },
    };
  }
}

export function parse(source: string): Document {
  const parser = new Parser(source);
  return parser.parse();
}
