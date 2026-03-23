import {
  concat,
  decodeOption,
  decodeU32,
  encodeOption,
  encodeU32,
  type DecodeResult,
} from "@bearcove/vox-postcard";

export interface PacketAck {
  max_delivered: number;
}

export interface StableFrame {
  seq: number;
  ack: PacketAck | null;
  item: Uint8Array;
}

function encodePacketAck(ack: PacketAck): Uint8Array {
  return encodeU32(ack.max_delivered);
}

function decodePacketAck(
  buf: Uint8Array,
  offset: number,
): DecodeResult<PacketAck> {
  const maxDelivered = decodeU32(buf, offset);
  return {
    value: { max_delivered: maxDelivered.value },
    next: maxDelivered.next,
  };
}

function encodePayloadBytes(bytes: Uint8Array): Uint8Array {
  const out = new Uint8Array(4 + bytes.length);
  const view = new DataView(out.buffer, out.byteOffset, out.byteLength);
  view.setUint32(0, bytes.length, true);
  out.set(bytes, 4);
  return out;
}

function decodePayloadBytes(
  buf: Uint8Array,
  offset: number,
): DecodeResult<Uint8Array> {
  if (offset + 4 > buf.length) {
    throw new Error("stable frame payload: eof");
  }

  const view = new DataView(buf.buffer, buf.byteOffset + offset, 4);
  const len = view.getUint32(0, true);
  const start = offset + 4;
  const end = start + len;
  if (end > buf.length) {
    throw new Error("stable frame payload: overrun");
  }

  return {
    value: buf.subarray(start, end),
    next: end,
  };
}

export function encodeStableFrame(frame: StableFrame): Uint8Array {
  return concat(
    encodeU32(frame.seq),
    encodeOption(frame.ack, encodePacketAck),
    encodePayloadBytes(frame.item),
  );
}

export function decodeStableFrame(
  buf: Uint8Array,
  offset: number,
): DecodeResult<StableFrame> {
  const seq = decodeU32(buf, offset);
  const ack = decodeOption(buf, seq.next, decodePacketAck);
  const item = decodePayloadBytes(buf, ack.next);
  return {
    value: {
      seq: seq.value,
      ack: ack.value,
      item: item.value,
    },
    next: item.next,
  };
}
