import { hexToBytes } from "@bearcove/phon-schema";
import { buildPlan, decodeTyped, encodeTyped } from "@bearcove/phon-engine";
import {
  parseSchemaClosure,
  messageRegistry,
  messageSchemaClosure,
  messageSchemaId,
  type Metadata,
  emptyMetadata,
  coerceMetadata,
} from "@bearcove/vox-wire";
import type { ConnectionSettings, Parity } from "@bearcove/vox-wire";
import type { Link } from "./link.ts";
import {
  registry,
  schemaId,
  handshakeSchemaClosure,
  type HandshakeMessage,
} from "./handshake.phon.generated.ts";

// Re-export Metadata for downstream consumers that used to import it from here.
export type { Metadata } from "@bearcove/vox-wire";

export interface HandshakeResult {
  localSettings: ConnectionSettings;
  peerSettings: ConnectionSettings;
  peerMessageSchema: Uint8Array;
  peerMetadata: Metadata;
}

// ---------------------------------------------------------------------------
// phon self-describing framing
//
// Each handshake message is sent as:
//   [u32 schema_len little-endian][schema-closure bytes][phon-compact value]
// ---------------------------------------------------------------------------

function encodeHandshake(msg: HandshakeMessage): Uint8Array {
  const value = encodeTyped(msg as never, schemaId.HandshakeMessage, registry);
  const closure = hexToBytes(handshakeSchemaClosure);

  const out = new Uint8Array(4 + closure.length + value.length);
  const dv = new DataView(out.buffer, out.byteOffset, out.byteLength);
  dv.setUint32(0, closure.length, true);
  out.set(closure, 4);
  out.set(value, 4 + closure.length);
  return out;
}

function decodeHandshake(bytes: Uint8Array): HandshakeMessage {
  const dv = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const len = dv.getUint32(0, true);
  const closure = bytes.subarray(4, 4 + len);
  const value = bytes.subarray(4 + len);
  const { root, schemas } = parseSchemaClosure(closure);
  return decodeTyped(
    value,
    root,
    schemaId.HandshakeMessage,
    registry.with(schemas),
  ) as unknown as HandshakeMessage;
}

async function recvHandshake(link: Link): Promise<HandshakeMessage> {
  const payload = await link.recv();
  if (!payload) {
    throw new Error("peer closed during handshake");
  }
  return decodeHandshake(payload);
}

async function sendHandshake(link: Link, message: HandshakeMessage): Promise<void> {
  await link.send(encodeHandshake(message));
}

const UNSUPPORTED_MESSAGE_COMPATIBILITY_PLAN = "unsupported message compatibility plan";

function peerMessageSchemaRejectionReason(peerSchema: Uint8Array): string | null {
  try {
    const { root, schemas } = parseSchemaClosure(peerSchema);
    buildPlan(root, messageSchemaId.Message, messageRegistry.with(schemas));
    return null;
  } catch {
    return UNSUPPORTED_MESSAGE_COMPATIBILITY_PLAN;
  }
}

async function sendSorryAndReject(link: Link, reason: string): Promise<never> {
  await sendHandshake(link, { tag: "Sorry", value: { reason } });
  throw new Error(reason);
}

function oppositeParity(parity: Parity): Parity {
  return parity.tag === "Odd" ? { tag: "Even" } : { tag: "Odd" };
}

// The sender's Message-envelope schema closure, sent verbatim as a byte list.
function localMessagePayloadSchema(): number[] {
  return Array.from(hexToBytes(messageSchemaClosure));
}

export async function handshakeAsInitiator(
  link: Link,
  settings: ConnectionSettings,
  metadata: Metadata = emptyMetadata(),
): Promise<HandshakeResult> {
  await sendHandshake(link, {
    tag: "Hello",
    value: {
      parity: settings.parity,
      connection_settings: settings,
      message_payload_schema: localMessagePayloadSchema(),
      metadata,
    },
  });

  const response = await recvHandshake(link);
  if (response.tag === "Sorry") {
    throw new Error(`handshake rejected: ${response.value.reason}`);
  }
  if (response.tag !== "HelloYourself") {
    throw new Error("expected HelloYourself during handshake");
  }

  const peerMessageSchema = new Uint8Array(response.value.message_payload_schema);
  const rejectionReason = peerMessageSchemaRejectionReason(peerMessageSchema);
  if (rejectionReason !== null) {
    await sendSorryAndReject(link, rejectionReason);
  }

  await sendHandshake(link, { tag: "LetsGo", value: {} });

  const helloYourself = response;
  const peerMetadata = coerceMetadata(helloYourself.value.metadata);
  return {
    localSettings: settings,
    peerSettings: helloYourself.value.connection_settings,
    peerMessageSchema,
    peerMetadata,
  };
}

export async function handshakeAsAcceptor(
  link: Link,
  settings: ConnectionSettings,
  metadata: Metadata = emptyMetadata(),
): Promise<HandshakeResult> {
  const first = await recvHandshake(link);
  if (first.tag !== "Hello") {
    throw new Error("expected Hello during handshake");
  }
  const hello = first;
  const peerMessageSchema = new Uint8Array(hello.value.message_payload_schema);
  const rejectionReason = peerMessageSchemaRejectionReason(peerMessageSchema);
  if (rejectionReason !== null) {
    await sendSorryAndReject(link, rejectionReason);
  }
  const localSettings = {
    ...settings,
    parity: oppositeParity(hello.value.parity),
  };

  await sendHandshake(link, {
    tag: "HelloYourself",
    value: {
      connection_settings: localSettings,
      message_payload_schema: localMessagePayloadSchema(),
      metadata,
    },
  });

  const third = await recvHandshake(link);
  if (third.tag === "Sorry") {
    throw new Error(`handshake rejected: ${third.value.reason}`);
  }
  if (third.tag !== "LetsGo") {
    throw new Error("expected LetsGo during handshake");
  }

  const peerMetadata = coerceMetadata(hello.value.metadata);
  return {
    localSettings,
    peerSettings: hello.value.connection_settings,
    peerMessageSchema,
    peerMetadata,
  };
}

export function voxServiceMetadata(serviceName: string): Metadata {
  const metadata: Metadata = new Map();
  metadata.set("vox-service", serviceName);
  return metadata;
}
