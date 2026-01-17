/** Byte offset span in source */
export interface Span {
  start: number;
  end: number;
}

/** Scalar kinds */
export type ScalarKind = "bare" | "quoted" | "raw" | "heredoc";

/** Separator style in objects */
export type Separator = "newline" | "comma";

/** A scalar value */
export interface Scalar {
  type: "scalar";
  text: string;
  kind: ScalarKind;
  span: Span;
}

/** A sequence of values */
export interface Sequence {
  type: "sequence";
  items: Value[];
  span: Span;
}

/** An object entry */
export interface Entry {
  key: Value;
  value: Value;
}

/** An object (key-value pairs) */
export interface StyxObject {
  type: "object";
  entries: Entry[];
  separator: Separator;
  span: Span;
}

/** A tag on a value */
export interface Tag {
  name: string;
  span: Span;
}

/** A Styx value */
export interface Value {
  tag?: Tag;
  payload?: Scalar | Sequence | StyxObject;
  span: Span;
}

/** Parse result */
export interface Document {
  entries: Entry[];
  span: Span;
}

/** Parse error */
export class ParseError extends Error {
  constructor(
    message: string,
    public span: Span
  ) {
    super(message);
    this.name = "ParseError";
  }
}
