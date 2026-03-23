import type { Schema } from "@bearcove/vox-postcard";
import type { ConnectionSettings, Parity } from "@bearcove/vox-wire";
import { messageSchemasCbor } from "@bearcove/vox-wire";
import { decodeCbor, type CborMap, type CborValue } from "./cbor.ts";
import type { Link } from "./link.ts";
import { normalizeSchemaList } from "./schema_cbor.ts";

export interface HandshakeResult {
  localSettings: ConnectionSettings;
  peerSettings: ConnectionSettings;
  peerSupportsRetry: boolean;
  sessionResumeKey: Uint8Array | null;
  peerResumeKey: Uint8Array | null;
  peerMessageSchema: Schema[];
}

type HandshakeMessage =
  | { tag: "Hello"; value: HelloMessage }
  | { tag: "HelloYourself"; value: HelloYourselfMessage }
  | { tag: "LetsGo" }
  | { tag: "Sorry"; reason: string };

interface HelloMessage {
  parity: Parity;
  connection_settings: ConnectionSettings;
  message_payload_schema: CborValue[];
  supports_retry: boolean;
  resume_key: Uint8Array | null;
}

interface HelloYourselfMessage {
  connection_settings: ConnectionSettings;
  message_payload_schema: CborValue[];
  supports_retry: boolean;
  resume_key: Uint8Array | null;
}

function concatChunks(chunks: Uint8Array[]): Uint8Array {
  let total = 0;
  for (const chunk of chunks) {
    total += chunk.length;
  }
  const out = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) {
    out.set(chunk, offset);
    offset += chunk.length;
  }
  return out;
}

function encodeMajor(major: number, value: number | bigint): Uint8Array {
  const big = typeof value === "bigint" ? value : BigInt(value);

  if (big < 24n) {
    return Uint8Array.of((major << 5) | Number(big));
  }
  if (big <= 0xffn) {
    return Uint8Array.of((major << 5) | 24, Number(big));
  }
  if (big <= 0xffffn) {
    return Uint8Array.of((major << 5) | 25, Number((big >> 8n) & 0xffn), Number(big & 0xffn));
  }
  if (big <= 0xffff_ffffn) {
    return Uint8Array.of(
      (major << 5) | 26,
      Number((big >> 24n) & 0xffn),
      Number((big >> 16n) & 0xffn),
      Number((big >> 8n) & 0xffn),
      Number(big & 0xffn),
    );
  }

  const out = new Uint8Array(9);
  out[0] = (major << 5) | 27;
  let remaining = big;
  for (let i = 8; i >= 1; i--) {
    out[i] = Number(remaining & 0xffn);
    remaining >>= 8n;
  }
  return out;
}

function encodeUint(value: number | bigint): Uint8Array {
  return encodeMajor(0, value);
}

function encodeBool(value: boolean): Uint8Array {
  return Uint8Array.of(value ? 0xf5 : 0xf4);
}

function encodeNull(): Uint8Array {
  return Uint8Array.of(0xf6);
}

function encodeBytes(value: Uint8Array): Uint8Array {
  return concatChunks([encodeMajor(2, value.length), value]);
}

function encodeText(value: string): Uint8Array {
  const encoded = new TextEncoder().encode(value);
  return concatChunks([encodeMajor(3, encoded.length), encoded]);
}

function encodeArray(items: Uint8Array[]): Uint8Array {
  return concatChunks([encodeMajor(4, items.length), ...items]);
}

function encodeMap(entries: Array<[string, Uint8Array]>): Uint8Array {
  const chunks: Uint8Array[] = [encodeMajor(5, entries.length)];
  for (const [key, value] of entries) {
    chunks.push(encodeText(key), value);
  }
  return concatChunks(chunks);
}

function encodeParity(parity: Parity): Uint8Array {
  switch (parity.tag) {
    case "Odd":
      return encodeMap([["Odd", encodeNull()]]);
    case "Even":
      return encodeMap([["Even", encodeNull()]]);
  }
}

function encodeConnectionSettings(settings: ConnectionSettings): Uint8Array {
  return encodeMap([
    ["parity", encodeParity(settings.parity)],
    ["max_concurrent_requests", encodeUint(settings.max_concurrent_requests)],
  ]);
}

function encodeResumeKey(resumeKey: Uint8Array | null): Uint8Array {
  if (resumeKey === null) {
    return encodeNull();
  }
  return encodeMap([["bytes", encodeBytes(resumeKey)]]);
}

function encodeHelloMessage(
  settings: ConnectionSettings,
  supportsRetry: boolean,
  resumeKey: Uint8Array | null,
): Uint8Array {
  return encodeMap([
    ["parity", encodeParity(settings.parity)],
    ["connection_settings", encodeConnectionSettings(settings)],
    ["message_payload_schema", messageSchemasCbor],
    ["supports_retry", encodeBool(supportsRetry)],
    ["resume_key", encodeResumeKey(resumeKey)],
  ]);
}

function encodeHelloYourselfMessage(
  settings: ConnectionSettings,
  supportsRetry: boolean,
  sessionResumeKey: Uint8Array | null,
): Uint8Array {
  return encodeMap([
    ["connection_settings", encodeConnectionSettings(settings)],
    ["message_payload_schema", messageSchemasCbor],
    ["supports_retry", encodeBool(supportsRetry)],
    ["resume_key", encodeResumeKey(sessionResumeKey)],
  ]);
}

function encodeHandshakeMessage(message: HandshakeMessage): Uint8Array {
  switch (message.tag) {
    case "Hello":
      return encodeMap([["Hello", encodeHelloMessage(message.value.connection_settings, message.value.supports_retry, message.value.resume_key)]]);
    case "HelloYourself":
      return encodeMap([
        [
          "HelloYourself",
          encodeHelloYourselfMessage(
            message.value.connection_settings,
            message.value.supports_retry,
            message.value.resume_key,
          ),
        ],
      ]);
    case "LetsGo":
      return encodeMap([["LetsGo", encodeMap([])]]);
    case "Sorry":
      return encodeMap([["Sorry", encodeMap([["reason", encodeText(message.reason)]])]]);
  }
}

function expectMap(value: CborValue, context: string): CborMap {
  if (
    value === null ||
    typeof value !== "object" ||
    value instanceof Uint8Array ||
    Array.isArray(value)
  ) {
    throw new Error(`expected map for ${context}`);
  }
  return value as CborMap;
}

function expectArray(value: CborValue, context: string): CborValue[] {
  if (!Array.isArray(value)) {
    throw new Error(`expected array for ${context}`);
  }
  return value;
}

function expectString(value: CborValue, context: string): string {
  if (typeof value !== "string") {
    throw new Error(`expected string for ${context}`);
  }
  return value;
}

function expectNumber(value: CborValue, context: string): number {
  if (typeof value !== "number") {
    throw new Error(`expected number for ${context}`);
  }
  return value;
}

function expectBool(value: CborValue, context: string): boolean {
  if (typeof value !== "boolean") {
    throw new Error(`expected boolean for ${context}`);
  }
  return value;
}

function parseParity(value: CborValue): Parity {
  const map = expectMap(value, "parity");
  const keys = Object.keys(map);
  if (keys.length !== 1) {
    throw new Error(`expected parity enum map with 1 entry, got ${keys.length}`);
  }
  const variant = keys[0];
  if (variant === "Odd") {
    return { tag: "Odd" };
  }
  if (variant === "Even") {
    return { tag: "Even" };
  }
  throw new Error(`unknown parity variant ${variant}`);
}

function parseConnectionSettings(value: CborValue): ConnectionSettings {
  const map = expectMap(value, "connection_settings");
  return {
    parity: parseParity(map["parity"]),
    max_concurrent_requests: expectNumber(
      map["max_concurrent_requests"],
      "connection_settings.max_concurrent_requests",
    ),
  };
}

function parseResumeKey(value: CborValue): Uint8Array | null {
  if (value === null) {
    return null;
  }
  const map = expectMap(value, "resume_key");
  const bytes = map["bytes"];
  if (!(bytes instanceof Uint8Array)) {
    throw new Error("expected byte string for resume_key.bytes");
  }
  return bytes.slice();
}

function parseHello(value: CborValue): HelloMessage {
  const map = expectMap(value, "Hello");
  return {
    parity: parseParity(map["parity"]),
    connection_settings: parseConnectionSettings(map["connection_settings"]),
    message_payload_schema: expectArray(map["message_payload_schema"], "Hello.message_payload_schema"),
    supports_retry:
      map["supports_retry"] === undefined
        ? false
        : expectBool(map["supports_retry"], "Hello.supports_retry"),
    resume_key:
      map["resume_key"] === undefined ? null : parseResumeKey(map["resume_key"]),
  };
}

function parseHelloYourself(value: CborValue): HelloYourselfMessage {
  const map = expectMap(value, "HelloYourself");
  return {
    connection_settings: parseConnectionSettings(map["connection_settings"]),
    message_payload_schema: expectArray(
      map["message_payload_schema"],
      "HelloYourself.message_payload_schema",
    ),
    supports_retry:
      map["supports_retry"] === undefined
        ? false
        : expectBool(map["supports_retry"], "HelloYourself.supports_retry"),
    resume_key:
      map["resume_key"] === undefined ? null : parseResumeKey(map["resume_key"]),
  };
}

function parseHandshakeMessage(bytes: Uint8Array): HandshakeMessage {
  const decoded = decodeCbor(bytes);
  const root = expectMap(decoded.value, "handshake message");
  const keys = Object.keys(root);
  if (keys.length !== 1) {
    throw new Error(`expected handshake enum map with 1 entry, got ${keys.length}`);
  }

  const tag = keys[0];
  const payload = root[tag];

  switch (tag) {
    case "Hello":
      return { tag: "Hello", value: parseHello(payload) };
    case "HelloYourself":
      return { tag: "HelloYourself", value: parseHelloYourself(payload) };
    case "LetsGo":
      return { tag: "LetsGo" };
    case "Sorry": {
      const map = expectMap(payload, "Sorry");
      return { tag: "Sorry", reason: expectString(map["reason"], "Sorry.reason") };
    }
    default:
      throw new Error(`unknown handshake message ${tag}`);
  }
}

async function recvHandshake(link: Link): Promise<HandshakeMessage> {
  const payload = await link.recv();
  if (!payload) {
    throw new Error("peer closed during handshake");
  }
  return parseHandshakeMessage(payload);
}

async function sendHandshake(link: Link, message: HandshakeMessage): Promise<void> {
  await link.send(encodeHandshakeMessage(message));
}

function sameBytes(left: Uint8Array, right: Uint8Array): boolean {
  if (left.length !== right.length) {
    return false;
  }
  for (let i = 0; i < left.length; i++) {
    if (left[i] !== right[i]) {
      return false;
    }
  }
  return true;
}

function randomSessionResumeKey(): Uint8Array {
  const bytes = new Uint8Array(16);
  const cryptoApi = globalThis.crypto;
  if (!cryptoApi) {
    throw new Error("crypto.getRandomValues is unavailable");
  }
  cryptoApi.getRandomValues(bytes);
  return bytes;
}

export async function handshakeAsInitiator(
  link: Link,
  settings: ConnectionSettings,
  supportsRetry: boolean = true,
  resumeKey: Uint8Array | null = null,
): Promise<HandshakeResult> {
  await sendHandshake(link, {
    tag: "Hello",
    value: {
      parity: settings.parity,
      connection_settings: settings,
      message_payload_schema: [],
      supports_retry: supportsRetry,
      resume_key: resumeKey,
    },
  });

  const response = await recvHandshake(link);
  if (response.tag === "Sorry") {
    throw new Error(`handshake rejected: ${response.reason}`);
  }
  if (response.tag !== "HelloYourself") {
    throw new Error("expected HelloYourself during handshake");
  }

  await sendHandshake(link, { tag: "LetsGo" });

  return {
    localSettings: settings,
    peerSettings: response.value.connection_settings,
    peerSupportsRetry: response.value.supports_retry,
    sessionResumeKey: response.value.resume_key,
    peerResumeKey: null,
    peerMessageSchema: normalizeSchemaList(response.value.message_payload_schema),
  };
}

export async function handshakeAsAcceptor(
  link: Link,
  settings: ConnectionSettings,
  supportsRetry: boolean = true,
  resumable: boolean = false,
  expectedResumeKey: Uint8Array | null = null,
): Promise<HandshakeResult> {
  const first = await recvHandshake(link);
  if (first.tag !== "Hello") {
    throw new Error("expected Hello during handshake");
  }

  if (expectedResumeKey) {
    const actual = first.value.resume_key;
    if (!actual || !sameBytes(actual, expectedResumeKey)) {
      await sendHandshake(link, {
        tag: "Sorry",
        reason: "session resume key mismatch",
      });
      throw new Error("session resume key mismatch");
    }
  }

  const sessionResumeKey = resumable ? randomSessionResumeKey() : null;
  await sendHandshake(link, {
    tag: "HelloYourself",
    value: {
      connection_settings: settings,
      message_payload_schema: [],
      supports_retry: supportsRetry,
      resume_key: sessionResumeKey,
    },
  });

  const third = await recvHandshake(link);
  if (third.tag === "Sorry") {
    throw new Error(`handshake rejected: ${third.reason}`);
  }
  if (third.tag !== "LetsGo") {
    throw new Error("expected LetsGo during handshake");
  }

  return {
    localSettings: settings,
    peerSettings: first.value.connection_settings,
    peerSupportsRetry: first.value.supports_retry,
    sessionResumeKey,
    peerResumeKey: first.value.resume_key,
    peerMessageSchema: normalizeSchemaList(first.value.message_payload_schema),
  };
}
