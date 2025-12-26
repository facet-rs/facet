/**
 * Frame - a complete message unit in the rapace protocol.
 *
 * Wire format:
 * [4 bytes: frame_len (LE)] [64 bytes: MsgDescHot] [payload bytes]
 *
 * The frame_len is the total length of descriptor + payload (NOT including the 4-byte length prefix).
 */

import { MsgDescHot, MSG_DESC_HOT_SIZE, INLINE_PAYLOAD_SIZE } from "./msg-desc-hot.js";
import { FrameFlags, setFlag } from "./frame-flags.js";

/** Minimum frame size (just the descriptor, no external payload). */
export const MIN_FRAME_SIZE = MSG_DESC_HOT_SIZE;

/**
 * A complete frame with descriptor and payload.
 */
export class Frame {
  /** The message descriptor. */
  desc: MsgDescHot;

  /** External payload (when not inline). */
  externalPayload: Uint8Array | null;

  constructor(desc?: MsgDescHot) {
    this.desc = desc ?? new MsgDescHot();
    this.externalPayload = null;
  }

  /**
   * Get the payload data, whether inline or external.
   */
  getPayload(): Uint8Array {
    if (this.desc.isInline) {
      return this.desc.getInlinePayloadData();
    }
    return this.externalPayload ?? new Uint8Array(0);
  }

  /**
   * Set the payload, automatically choosing inline vs external.
   */
  setPayload(data: Uint8Array): void {
    if (data.length <= INLINE_PAYLOAD_SIZE) {
      this.desc.setInlinePayload(data);
      this.externalPayload = null;
    } else {
      this.desc.payloadSlot = 0; // External payload marker
      this.desc.payloadOffset = 0;
      this.desc.payloadLen = data.length;
      this.externalPayload = data;
    }
  }

  /**
   * Serialize the frame for transmission.
   *
   * Returns: [4-byte length LE][64-byte descriptor][payload]
   */
  serialize(): Uint8Array {
    const descBytes = this.desc.serialize();
    const payloadBytes = this.desc.isInline ? new Uint8Array(0) : (this.externalPayload ?? new Uint8Array(0));

    const frameLen = descBytes.length + payloadBytes.length;
    const result = new Uint8Array(4 + frameLen);
    const view = new DataView(result.buffer);

    // 4-byte length prefix
    view.setUint32(0, frameLen, true);

    // 64-byte descriptor
    result.set(descBytes, 4);

    // External payload (if any)
    if (payloadBytes.length > 0) {
      result.set(payloadBytes, 4 + MSG_DESC_HOT_SIZE);
    }

    return result;
  }

  /**
   * Parse a frame from raw bytes.
   *
   * @param data - Complete frame data including length prefix
   */
  static parse(data: Uint8Array): Frame {
    if (data.length < 4) {
      throw new Error("Frame too short: missing length prefix");
    }

    const view = new DataView(data.buffer, data.byteOffset, data.byteLength);
    const frameLen = view.getUint32(0, true);

    if (data.length < 4 + frameLen) {
      throw new Error(`Frame incomplete: expected ${4 + frameLen} bytes, got ${data.length}`);
    }

    if (frameLen < MSG_DESC_HOT_SIZE) {
      throw new Error(`Frame too short: frameLen ${frameLen} < ${MSG_DESC_HOT_SIZE}`);
    }

    const descBytes = data.slice(4, 4 + MSG_DESC_HOT_SIZE);
    const desc = MsgDescHot.parse(descBytes);

    const frame = new Frame(desc);

    // If not inline and there's external payload
    if (!desc.isInline && frameLen > MSG_DESC_HOT_SIZE) {
      frame.externalPayload = data.slice(4 + MSG_DESC_HOT_SIZE, 4 + frameLen);
    }

    return frame;
  }

  /**
   * Create a data frame with the given payload.
   */
  static data(msgId: bigint, channelId: number, methodId: number, payload: Uint8Array): Frame {
    const frame = new Frame();
    frame.desc.msgId = msgId;
    frame.desc.channelId = channelId;
    frame.desc.methodId = methodId;
    frame.desc.flags = setFlag(0, FrameFlags.DATA);
    frame.setPayload(payload);
    return frame;
  }

  /**
   * Create an error frame.
   */
  static error(msgId: bigint, channelId: number, methodId: number, payload: Uint8Array): Frame {
    const frame = Frame.data(msgId, channelId, methodId, payload);
    frame.desc.flags = setFlag(frame.desc.flags, FrameFlags.ERROR);
    return frame;
  }
}
