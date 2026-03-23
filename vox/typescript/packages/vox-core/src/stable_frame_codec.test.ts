import { describe, expect, it } from "vitest";

import {
  decodeStableFrame,
  encodeStableFrame,
  type StableFrame,
} from "./stable_frame_codec.ts";

describe("stable frame codec", () => {
  it("roundtrips frames without acknowledgements", () => {
    const frame: StableFrame = {
      seq: 7,
      ack: null,
      item: new Uint8Array([1, 2, 3]),
    };

    const encoded = encodeStableFrame(frame);
    expect(Array.from(encoded)).toEqual([7, 0, 3, 0, 0, 0, 1, 2, 3]);

    const decoded = decodeStableFrame(encoded, 0);
    expect(decoded.next).toBe(encoded.length);
    expect(decoded.value).toEqual(frame);
  });

  it("roundtrips frames with acknowledgements", () => {
    const frame: StableFrame = {
      seq: 129,
      ack: { max_delivered: 42 },
      item: new Uint8Array([0xaa, 0xbb]),
    };

    const encoded = encodeStableFrame(frame);
    const decoded = decodeStableFrame(encoded, 0);
    expect(decoded.next).toBe(encoded.length);
    expect(decoded.value).toEqual(frame);
  });

  it("rejects truncated payload bytes", () => {
    expect(() =>
      decodeStableFrame(Uint8Array.of(1, 0, 4, 0, 0, 0, 0xaa, 0xbb), 0),
    ).toThrow("stable frame payload: overrun");
  });
});
