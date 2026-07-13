// Shared highlight helpers: byte/string offset mapping, capture class names, and
// non-overlapping capture selection. Used both by the CodeMirror editor (to build
// mark decorations) and by the captures list in the results dock.

export interface CaptureRange {
  capture_name: string;
  start_byte: number;
  end_byte: number;
  priority?: number;
}

export interface SelectedCapture<T extends CaptureRange> {
  capture: T;
  from: number;
  to: number;
}

/**
 * Maps every byte offset of `input` to its UTF-16 string index, which is also a
 * CodeMirror document position. The map has `byteLength + 1` entries so the
 * end-of-input offset resolves too.
 */
export function byteOffsetMap(input: string): number[] {
  const encoder = new TextEncoder();
  const totalBytes = encoder.encode(input).length;
  const map = new Array<number>(totalBytes + 1);
  let byteOffset = 0;
  let stringIndex = 0;
  map[0] = 0;
  for (const char of input) {
    const nextByteOffset = byteOffset + encoder.encode(char).length;
    const nextStringIndex = stringIndex + char.length;
    for (let byte = byteOffset; byte < nextByteOffset; byte += 1) {
      map[byte] = stringIndex;
    }
    map[nextByteOffset] = nextStringIndex;
    byteOffset = nextByteOffset;
    stringIndex = nextStringIndex;
  }
  return map;
}

/**
 * Resolves capture byte ranges to string ranges and drops overlaps, keeping the
 * earliest/longest capture at each position. Returns ranges sorted by `from`.
 */
export function selectNonOverlapping<T extends CaptureRange>(
  captures: T[],
  byteToStringIndex: number[],
  inputLength: number,
): SelectedCapture<T>[] {
  const selected = captures
    .map((capture) => ({
      capture,
      from: byteToStringIndex[capture.start_byte] ?? inputLength,
      to: byteToStringIndex[capture.end_byte] ?? inputLength,
    }))
    .filter((entry) => entry.from < entry.to)
    .sort((left, right) => {
      const leftPriority = left.capture.priority ?? 0;
      const rightPriority = right.capture.priority ?? 0;
      if (leftPriority !== rightPriority) {
        return rightPriority - leftPriority;
      }
      if (left.from !== right.from) {
        return left.from - right.from;
      }
      if (left.to !== right.to) {
        return right.to - left.to;
      }
      return left.capture.capture_name.localeCompare(right.capture.capture_name);
    })
    .reduce<SelectedCapture<T>[]>((selected, entry) => {
      if (!selected.some((kept) => rangesOverlap(kept.from, kept.to, entry.from, entry.to))) {
        selected.push(entry);
      }
      return selected;
    }, [])
    .sort((left, right) => left.from - right.from || left.to - right.to);
  return selected;
}

function rangesOverlap(leftFrom: number, leftTo: number, rightFrom: number, rightTo: number) {
  return leftFrom < rightTo && rightFrom < leftTo;
}

export function captureClass(captureName: string): string {
  const first = captureName.split(".")[0] ?? captureName;
  switch (first) {
    case "attribute":
    case "property":
      return "capture-property";
    case "comment":
      return "capture-comment";
    case "constant":
    case "number":
      return "capture-number";
    case "function":
    case "method":
      return "capture-function";
    case "keyword":
    case "operator":
      return "capture-keyword";
    case "punctuation":
      return "capture-punctuation";
    case "string":
      return "capture-string";
    case "type":
    case "constructor":
      return "capture-type";
    case "variable":
      return "capture-variable";
    default:
      return "capture-default";
  }
}
