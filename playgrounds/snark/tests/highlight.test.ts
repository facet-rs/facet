import assert from "node:assert/strict";
import test from "node:test";

import { byteOffsetMap, selectNonOverlapping } from "../src/highlight.ts";

test("keeps higher-priority nested captures over enclosing host captures", () => {
  const input = "alpha beta";
  const selected = selectNonOverlapping(
    [
      { capture_name: "string", start_byte: 0, end_byte: 10, priority: 0 },
      { capture_name: "variable", start_byte: 6, end_byte: 10, priority: 1 },
    ],
    byteOffsetMap(input),
    input.length,
  );

  assert.deepEqual(
    selected.map((entry) => ({
      capture: entry.capture.capture_name,
      from: entry.from,
      to: entry.to,
    })),
    [{ capture: "variable", from: 6, to: 10 }],
  );
});

test("preserves earliest-longest ordering for captures with equal priority", () => {
  const input = "alpha beta";
  const selected = selectNonOverlapping(
    [
      { capture_name: "variable", start_byte: 6, end_byte: 10 },
      { capture_name: "string", start_byte: 0, end_byte: 10 },
    ],
    byteOffsetMap(input),
    input.length,
  );

  assert.deepEqual(
    selected.map((entry) => entry.capture.capture_name),
    ["string"],
  );
});
