import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { decodeVarintNumber, decodeString, decodeU32, encodeU16, encodeU32, encodeU64 } from "@bearcove/roam-postcard";
import {
  type Message,
  MetadataFlags,
  parityOdd,
  parityEven,
  connectionSettings,
  helloV7,
  helloYourself,
  metadataString,
  metadataBytes,
  metadataU64,
  metadataEntry,
  messageHello,
  messageHelloYourself,
  messageProtocolError,
  messageConnect,
  messageAccept,
  messageReject,
  messageGoodbye,
  messageRequest,
  messageResponse,
  messageCancel,
  messageData,
  messageClose,
  messageReset,
  messageCredit,
  encodeMessage,
  decodeMessage,
} from "./index.ts";
import { RpcError, RpcErrorCode } from "./rpc_error.ts";

const __dirname = dirname(fileURLToPath(import.meta.url));

function loadGoldenVector(path: string): Uint8Array {
  const projectRoot = join(__dirname, "..", "..", "..", "..");
  return new Uint8Array(readFileSync(join(projectRoot, "test-fixtures", "golden-vectors", path)));
}

function sampleMetadata() {
  return [
    metadataEntry("trace-id", metadataString("abc123"), MetadataFlags.NONE),
    metadataEntry(
      "auth",
      metadataBytes(new Uint8Array([0xde, 0xad, 0xbe, 0xef])),
      MetadataFlags.SENSITIVE | MetadataFlags.NO_PROPAGATE,
    ),
    metadataEntry("attempt", metadataU64(2n), MetadataFlags.NONE),
  ];
}

function expectedMessages(): Array<[name: string, message: Message]> {
  const meta = sampleMetadata();

  return [
    ["message_hello", messageHello(helloV7(parityOdd(), 64, meta))],
    [
      "message_hello_yourself",
      messageHelloYourself(helloYourself(parityEven(), 32, meta)),
    ],
    ["message_protocol_error", messageProtocolError("bad frame sequence")],
    ["message_connection_open", messageConnect(2n, connectionSettings(parityOdd(), 64), meta)],
    ["message_connection_accept", messageAccept(2n, connectionSettings(parityEven(), 96), meta)],
    ["message_connection_reject", messageReject(4n, meta)],
    ["message_connection_close", messageGoodbye(2n, meta)],
    [
      "message_request_call",
      messageRequest(11n, 0xE5A1_D6B2_C390_F001n, encodeU32(0x1234_5678), meta, [3n, 5n], 2n),
    ],
    [
      "message_request_response",
      messageResponse(11n, encodeU64(0xFACE_B00Cn), meta, [7n], 2n),
    ],
    ["message_request_cancel", messageCancel(11n, 2n, meta)],
    ["message_channel_item", messageData(3n, encodeU16(77), 2n)],
    ["message_channel_close", messageClose(3n, 2n, meta)],
    ["message_channel_reset", messageReset(3n, 2n, meta)],
    ["message_channel_grant_credit", messageCredit(3n, 1024, 2n)],
  ];
}

describe("wire-v7 golden vectors", () => {
  it("encodes bytes matching Rust fixtures", () => {
    for (const [name, message] of expectedMessages()) {
      const encoded = encodeMessage(message);
      const expected = loadGoldenVector(`wire-v7/${name}.bin`);
      expect(Array.from(encoded), name).toEqual(Array.from(expected));
    }
  });

  it("decodes Rust fixtures into expected messages", () => {
    for (const [name, expectedMessage] of expectedMessages()) {
      const bytes = loadGoldenVector(`wire-v7/${name}.bin`);
      const decoded = decodeMessage(bytes);
      expect(decoded.next, name).toBe(bytes.length);
      expect(decoded.value, name).toEqual(expectedMessage);
    }
  });
});

// Mirrors the decode logic in connection.ts
function decodeOkString(bytes: Uint8Array): string {
  if (bytes[0] !== 0) throw new Error("expected Ok");
  return decodeString(bytes, 1).value;
}

function decodeOkU32(bytes: Uint8Array): number {
  if (bytes[0] !== 0) throw new Error("expected Ok");
  return decodeU32(bytes, 1).value;
}

function decodeErr(bytes: Uint8Array): RpcError {
  if (bytes[0] !== 1) throw new Error("expected Err");
  const variant = decodeVarintNumber(bytes, 1);
  if (variant.value === RpcErrorCode.USER) {
    return new RpcError(RpcErrorCode.USER, bytes.slice(variant.next));
  }
  return new RpcError(variant.value as RpcErrorCode);
}

describe("Result/RoamError golden vectors", () => {
  it("ok_string: [0x00, len, ...bytes]", () => {
    const bytes = loadGoldenVector("result/ok_string.bin");
    expect(Array.from(bytes)).toEqual([0x00, 0x05, 0x68, 0x65, 0x6c, 0x6c, 0x6f]);
    expect(decodeOkString(bytes)).toBe("hello");
  });

  it("ok_u32: [0x00, varint(42)]", () => {
    const bytes = loadGoldenVector("result/ok_u32.bin");
    expect(Array.from(bytes)).toEqual([0x00, 0x2a]);
    expect(decodeOkU32(bytes)).toBe(42);
  });

  it("err_unknown_method: [0x01, 0x01]", () => {
    const bytes = loadGoldenVector("result/err_unknown_method.bin");
    expect(Array.from(bytes)).toEqual([0x01, 0x01]);
    const err = decodeErr(bytes);
    expect(err.code).toBe(RpcErrorCode.UNKNOWN_METHOD);
    expect(err.payload).toBeNull();
  });

  it("err_invalid_payload: [0x01, 0x02]", () => {
    const bytes = loadGoldenVector("result/err_invalid_payload.bin");
    expect(Array.from(bytes)).toEqual([0x01, 0x02]);
    const err = decodeErr(bytes);
    expect(err.code).toBe(RpcErrorCode.INVALID_PAYLOAD);
    expect(err.payload).toBeNull();
  });

  it("err_cancelled: [0x01, 0x03]", () => {
    const bytes = loadGoldenVector("result/err_cancelled.bin");
    expect(Array.from(bytes)).toEqual([0x01, 0x03]);
    const err = decodeErr(bytes);
    expect(err.code).toBe(RpcErrorCode.CANCELLED);
    expect(err.payload).toBeNull();
  });

  it("err_user_string: [0x01, 0x00, len, ...bytes]", () => {
    const bytes = loadGoldenVector("result/err_user_string.bin");
    expect(Array.from(bytes)).toEqual([0x01, 0x00, 0x04, 0x6f, 0x6f, 0x70, 0x73]);
    const err = decodeErr(bytes);
    expect(err.code).toBe(RpcErrorCode.USER);
    expect(err.payload).not.toBeNull();
    expect(decodeString(err.payload!, 0).value).toBe("oops");
  });
});
