/**
 * Hot-path message descriptor (64 bytes, one cache line).
 *
 * This is the primary descriptor used for frame dispatch.
 * Fits in a single cache line for performance.
 *
 * Binary layout (all little-endian):
 * ```
 * offset  size  field
 * 0       8     msgId: u64
 * 8       4     channelId: u32
 * 12      4     methodId: u32
 * 16      4     payloadSlot: u32
 * 20      4     payloadGeneration: u32
 * 24      4     payloadOffset: u32
 * 28      4     payloadLen: u32
 * 32      4     flags: u32 (FrameFlags bitfield)
 * 36      4     creditGrant: u32
 * 40      8     deadlineNs: u64
 * 48      16    inlinePayload: [u8; 16] (fixed 16 bytes)
 * ```
 */

import { FrameFlags, FrameFlagsType, hasFlag } from "./frame-flags.js";

/** Size of inline payload in bytes. */
export const INLINE_PAYLOAD_SIZE = 16;

/** Sentinel value indicating payload is inline (not in a slot). */
export const INLINE_PAYLOAD_SLOT = 0xffffffff;

/** Sentinel value indicating no deadline. */
export const NO_DEADLINE = 0xffffffffffffffffn;

/** Size of the serialized descriptor in bytes. */
export const MSG_DESC_HOT_SIZE = 64;

export class MsgDescHotError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "MsgDescHotError";
  }
}

/**
 * Hot-path message descriptor.
 */
export class MsgDescHot {
  // Identity (16 bytes)
  /** Unique message ID per session, monotonic. */
  msgId: bigint = 0n;
  /** Logical stream (0 = control channel). */
  channelId: number = 0;
  /** For RPC dispatch, or control verb. */
  methodId: number = 0;

  // Payload location (16 bytes)
  /** Slot index (0xFFFFFFFF = inline). */
  payloadSlot: number = INLINE_PAYLOAD_SLOT;
  /** Generation counter for ABA safety. */
  payloadGeneration: number = 0;
  /** Offset within slot. */
  payloadOffset: number = 0;
  /** Actual payload length. */
  payloadLen: number = 0;

  // Flow control & timing (16 bytes)
  /** Frame flags (EOS, CANCEL, ERROR, etc.). */
  flags: FrameFlagsType = 0;
  /** Credits being granted to peer. */
  creditGrant: number = 0;
  /** Deadline in nanoseconds (monotonic clock). NO_DEADLINE = no deadline. */
  deadlineNs: bigint = NO_DEADLINE;

  // Inline payload for small messages (16 bytes)
  /** When payloadSlot == 0xFFFFFFFF, payload lives here. */
  inlinePayload: Uint8Array = new Uint8Array(INLINE_PAYLOAD_SIZE);

  /**
   * Parse from 64 bytes of raw data.
   */
  static parse(data: Uint8Array): MsgDescHot {
    if (data.length !== MSG_DESC_HOT_SIZE) {
      throw new MsgDescHotError(
        `Invalid size: expected ${MSG_DESC_HOT_SIZE}, got ${data.length}`
      );
    }

    const view = new DataView(data.buffer, data.byteOffset, data.byteLength);
    const desc = new MsgDescHot();

    // Identity (16 bytes)
    desc.msgId = view.getBigUint64(0, true);
    desc.channelId = view.getUint32(8, true);
    desc.methodId = view.getUint32(12, true);

    // Payload location (16 bytes)
    desc.payloadSlot = view.getUint32(16, true);
    desc.payloadGeneration = view.getUint32(20, true);
    desc.payloadOffset = view.getUint32(24, true);
    desc.payloadLen = view.getUint32(28, true);

    // Flow control & timing (16 bytes)
    desc.flags = view.getUint32(32, true);
    desc.creditGrant = view.getUint32(36, true);
    desc.deadlineNs = view.getBigUint64(40, true);

    // Inline payload (16 bytes)
    desc.inlinePayload = data.slice(48, 64);

    return desc;
  }

  /**
   * Serialize to 64 bytes of raw data.
   */
  serialize(): Uint8Array {
    const data = new Uint8Array(MSG_DESC_HOT_SIZE);
    const view = new DataView(data.buffer);

    // Identity (16 bytes)
    view.setBigUint64(0, this.msgId, true);
    view.setUint32(8, this.channelId, true);
    view.setUint32(12, this.methodId, true);

    // Payload location (16 bytes)
    view.setUint32(16, this.payloadSlot, true);
    view.setUint32(20, this.payloadGeneration, true);
    view.setUint32(24, this.payloadOffset, true);
    view.setUint32(28, this.payloadLen, true);

    // Flow control & timing (16 bytes)
    view.setUint32(32, this.flags, true);
    view.setUint32(36, this.creditGrant, true);
    view.setBigUint64(40, this.deadlineNs, true);

    // Inline payload (16 bytes)
    data.set(this.inlinePayload.subarray(0, INLINE_PAYLOAD_SIZE), 48);

    return data;
  }

  /**
   * Returns true if this frame has a deadline set.
   */
  get hasDeadline(): boolean {
    return this.deadlineNs !== NO_DEADLINE;
  }

  /**
   * Returns true if payload is inline (not in a slot).
   */
  get isInline(): boolean {
    return this.payloadSlot === INLINE_PAYLOAD_SLOT;
  }

  /**
   * Returns true if this is a control frame (channel 0).
   */
  get isControl(): boolean {
    return this.channelId === 0;
  }

  /**
   * Returns true if this is a data frame.
   */
  get isData(): boolean {
    return hasFlag(this.flags, FrameFlags.DATA);
  }

  /**
   * Returns true if this is an error response.
   */
  get isError(): boolean {
    return hasFlag(this.flags, FrameFlags.ERROR);
  }

  /**
   * Returns true if this is end of stream.
   */
  get isEos(): boolean {
    return hasFlag(this.flags, FrameFlags.EOS);
  }

  /**
   * Get inline payload data (only valid if isInline).
   */
  getInlinePayloadData(): Uint8Array {
    return this.inlinePayload.subarray(0, this.payloadLen);
  }

  /**
   * Set inline payload from data.
   *
   * @param data - Up to 16 bytes of payload data.
   */
  setInlinePayload(data: Uint8Array): void {
    if (data.length > INLINE_PAYLOAD_SIZE) {
      throw new MsgDescHotError(
        `Inline payload must be at most ${INLINE_PAYLOAD_SIZE} bytes, got ${data.length}`
      );
    }

    // Zero-initialize
    this.inlinePayload.fill(0);

    // Copy data
    this.inlinePayload.set(data);

    this.payloadLen = data.length;
    this.payloadSlot = INLINE_PAYLOAD_SLOT;
  }

  /**
   * Create a debug string representation.
   */
  toString(): string {
    const slotStr =
      this.payloadSlot === INLINE_PAYLOAD_SLOT
        ? "INLINE"
        : String(this.payloadSlot);
    const deadlineStr =
      this.deadlineNs === NO_DEADLINE ? "NONE" : String(this.deadlineNs);

    return `MsgDescHot {
  msgId: ${this.msgId}
  channelId: ${this.channelId}
  methodId: 0x${this.methodId.toString(16).padStart(8, "0")}
  payloadSlot: ${slotStr}
  payloadGeneration: ${this.payloadGeneration}
  payloadOffset: ${this.payloadOffset}
  payloadLen: ${this.payloadLen}
  flags: 0x${this.flags.toString(16)}
  creditGrant: ${this.creditGrant}
  deadlineNs: ${deadlineStr}
  isInline: ${this.isInline}
}`;
  }
}
