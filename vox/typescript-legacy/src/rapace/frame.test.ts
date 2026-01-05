import { describe, it } from "node:test";
import * as assert from "node:assert";
import { Frame } from "./frame.js";
import { MsgDescHot, MSG_DESC_HOT_SIZE } from "./msg-desc-hot.js";
import { FrameFlags } from "./frame-flags.js";

describe("Frame", () => {
  it("serializes and parses inline payload", () => {
    const frame = Frame.data(1n, 1, 0x12345678, new Uint8Array([1, 2, 3, 4]));

    const serialized = frame.serialize();

    // Should be: 4 bytes length + 64 bytes descriptor
    // Inline payload doesn't add to the wire size
    assert.strictEqual(serialized.length, 4 + MSG_DESC_HOT_SIZE);

    const parsed = Frame.parse(serialized);
    assert.strictEqual(parsed.desc.msgId, 1n);
    assert.strictEqual(parsed.desc.channelId, 1);
    assert.strictEqual(parsed.desc.methodId, 0x12345678);
    assert.deepStrictEqual(parsed.getPayload(), new Uint8Array([1, 2, 3, 4]));
  });

  it("serializes and parses external payload", () => {
    // Create a payload larger than 16 bytes
    const payload = new Uint8Array(32);
    for (let i = 0; i < 32; i++) {
      payload[i] = i;
    }

    const frame = Frame.data(2n, 2, 0xabcdef00, payload);

    const serialized = frame.serialize();

    // Should be: 4 bytes length + 64 bytes descriptor + 32 bytes payload
    assert.strictEqual(serialized.length, 4 + MSG_DESC_HOT_SIZE + 32);

    const parsed = Frame.parse(serialized);
    assert.strictEqual(parsed.desc.msgId, 2n);
    assert.strictEqual(parsed.desc.channelId, 2);
    assert.strictEqual(parsed.desc.methodId, 0xabcdef00);
    assert.deepStrictEqual(parsed.getPayload(), payload);
  });

  it("creates error frames", () => {
    const frame = Frame.error(3n, 3, 0x11111111, new Uint8Array([0xff]));
    assert.ok(frame.desc.isError);
  });
});

describe("MsgDescHot", () => {
  it("serializes and parses correctly", () => {
    const desc = new MsgDescHot();
    desc.msgId = 12345n;
    desc.channelId = 100;
    desc.methodId = 0xdeadbeef;
    desc.flags = FrameFlags.DATA | FrameFlags.HIGH_PRIORITY;
    desc.creditGrant = 1000;

    const serialized = desc.serialize();
    assert.strictEqual(serialized.length, MSG_DESC_HOT_SIZE);

    const parsed = MsgDescHot.parse(serialized);
    assert.strictEqual(parsed.msgId, 12345n);
    assert.strictEqual(parsed.channelId, 100);
    assert.strictEqual(parsed.methodId, 0xdeadbeef);
    assert.strictEqual(parsed.flags, FrameFlags.DATA | FrameFlags.HIGH_PRIORITY);
    assert.strictEqual(parsed.creditGrant, 1000);
  });

  it("handles inline payload", () => {
    const desc = new MsgDescHot();
    desc.setInlinePayload(new Uint8Array([10, 20, 30]));

    assert.ok(desc.isInline);
    assert.strictEqual(desc.payloadLen, 3);
    assert.deepStrictEqual(
      desc.getInlinePayloadData(),
      new Uint8Array([10, 20, 30])
    );
  });
});
