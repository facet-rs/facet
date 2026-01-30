import { Value, Scalar, Sequence, StyxObject, Entry, Document, Span } from "./types.js";

function jsonEscape(s: string): string {
  let result = "";
  for (const ch of s) {
    switch (ch) {
      case '"':
        result += '\\"';
        break;
      case "\\":
        result += "\\\\";
        break;
      case "\n":
        result += "\\n";
        break;
      case "\r":
        result += "\\r";
        break;
      case "\t":
        result += "\\t";
        break;
      default:
        if (ch.charCodeAt(0) < 32) {
          result += `\\u${ch.charCodeAt(0).toString(16).padStart(4, "0")}`;
        } else {
          result += ch;
        }
    }
  }
  return result;
}

function spanStr(span: Span): string {
  return `[${span.start}, ${span.end}]`;
}

function indent(level: number): string {
  return "  ".repeat(level);
}

export function documentToSexp(doc: Document): string {
  const lines: string[] = [];
  lines.push(`(document [-1, -1]`);

  for (const entry of doc.entries) {
    lines.push(entryToSexp(entry, 1));
  }

  lines.push(")");
  return lines.join("\n");
}

function entryToSexp(entry: Entry, level: number): string {
  const pad = indent(level);
  const lines: string[] = [];
  lines.push(`${pad}(entry`);
  lines.push(valueToSexp(entry.key, level + 1));
  lines.push(valueToSexp(entry.value, level + 1) + ")");
  return lines.join("\n");
}

function valueToSexp(value: Value, level: number): string {
  const pad = indent(level);
  const span = spanStr(value.span);

  // Unit value
  if (!value.tag && !value.payload) {
    return `${pad}(unit ${span})`;
  }

  // Tagged value
  if (value.tag) {
    const tagName = jsonEscape(value.tag.name);
    if (value.payload) {
      const payloadSexp = payloadToSexp(value.payload, level + 1);
      return `${pad}(tag ${span} "${tagName}"\n${payloadSexp})`;
    } else {
      return `${pad}(tag ${span} "${tagName}")`;
    }
  }

  // Untagged value with payload
  if (value.payload) {
    return payloadToSexp(value.payload, level);
  }

  return `${pad}(unit ${span})`;
}

function payloadToSexp(payload: Scalar | Sequence | StyxObject, level: number): string {
  const pad = indent(level);

  if (payload.type === "scalar") {
    const span = spanStr(payload.span);
    const text = jsonEscape(payload.text);
    return `${pad}(scalar ${span} ${payload.kind} "${text}")`;
  }

  if (payload.type === "sequence") {
    const span = spanStr(payload.span);
    if (payload.items.length === 0) {
      return `${pad}(sequence ${span})`;
    }
    const lines: string[] = [];
    lines.push(`${pad}(sequence ${span}`);
    for (const item of payload.items) {
      lines.push(valueToSexp(item, level + 1));
    }
    lines.push(`${pad.slice(2)})`);
    // Remove extra closing on last item and close here
    const result = lines.join("\n");
    // Fix: close sequence properly
    return result.replace(/\n\s*\)$/, ")");
  }

  if (payload.type === "object") {
    const span = spanStr(payload.span);
    if (payload.entries.length === 0) {
      return `${pad}(object ${span})`;
    }
    const lines: string[] = [];
    lines.push(`${pad}(object ${span}`);
    for (const entry of payload.entries) {
      lines.push(entryToSexp(entry, level + 1));
    }
    lines.push(`${pad})`);
    return lines.join("\n");
  }

  throw new Error(`Unknown payload type`);
}

export function errorToSexp(message: string, span: Span): string {
  const fullMsg = `parse error at ${span.start}-${span.end}: ${message}`;
  const msg = jsonEscape(fullMsg);
  return `(error [${span.start}, ${span.end}] "${msg}")`;
}
