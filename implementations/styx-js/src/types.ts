/** Byte offset span in source */
export interface Span {
  start: number;
  end: number;
}

/** Scalar kinds */
export type ScalarKind = "bare" | "quoted" | "raw" | "heredoc";

/** Path value kind for tracking */
export type PathValueKind = "object" | "terminal";

/** Track path state for detecting reopen-path and nest-into-terminal errors */
export class PathState {
  currentPath: string[] = [];
  closedPaths = new Set<string>();
  assignedPaths = new Map<string, { kind: PathValueKind; span: Span }>();

  checkAndUpdate(path: string[], span: Span, kind: PathValueKind): void {
    const fullPath = path.join(".");

    // 1. Check for duplicate
    const existing = this.assignedPaths.get(fullPath);
    if (existing) {
      if (existing.kind === "terminal") {
        throw new ParseError("duplicate key", span);
      }
      // Both are objects - it's a reopen attempt
      throw new ParseError(`cannot reopen path \`${fullPath}\` after sibling appeared`, span);
    }

    // 2. Check if any prefix is closed (has had siblings) or is terminal
    for (let i = 1; i < path.length; i++) {
      const prefix = path.slice(0, i).join(".");
      if (this.closedPaths.has(prefix)) {
        throw new ParseError(`cannot reopen path \`${prefix}\` after sibling appeared`, span);
      }
      const prefixEntry = this.assignedPaths.get(prefix);
      if (prefixEntry && prefixEntry.kind === "terminal") {
        throw new ParseError(`cannot nest into \`${prefix}\` which has a terminal value`, span);
      }
    }

    // 3. Find common prefix length with current path
    let commonLen = 0;
    for (let i = 0; i < Math.min(path.length, this.currentPath.length); i++) {
      if (path[i] === this.currentPath[i]) {
        commonLen++;
      } else {
        break;
      }
    }

    // 4. Close all divergent paths from current path
    for (let i = commonLen; i < this.currentPath.length; i++) {
      const divergent = this.currentPath.slice(0, i + 1).join(".");
      this.closedPaths.add(divergent);
    }

    // 5. Record intermediate segments as objects
    for (let i = 0; i < path.length - 1; i++) {
      const prefix = path.slice(0, i + 1).join(".");
      if (!this.assignedPaths.has(prefix)) {
        this.assignedPaths.set(prefix, { kind: "object", span });
      }
    }

    // 6. Record the final path
    this.assignedPaths.set(fullPath, { kind, span });
    this.currentPath = [...path];
  }
}

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
    public span: Span,
  ) {
    super(message);
    this.name = "ParseError";
  }
}
