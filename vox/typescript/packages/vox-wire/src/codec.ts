// Wire codec for the vox `Message` envelope, on the phon engine.
//
// The envelope is an evolvable wire type like any other: decode uses the
// peer's `Message` schema (exchanged in the handshake) against our own via phon's
// compatibility plan. With no peer schema it degenerates to writer==reader — the
// same plan, not a shortcut.
// r[impl conduit.typeplan]
// r[impl schema.type-id]

import {
  type Registry,
  type Schema,
  schemaFromBytes,
  hexToBytes,
} from "@bearcove/phon-schema";
import { decodeTyped, encodeTyped } from "@bearcove/phon-engine";

import type { Message } from "./types.ts";
import { registry, schemaId } from "./wire.phon.generated.ts";

/** Encode a `Message` to phon-compact bytes against our local envelope schema. */
export function encodeMessage(message: Message): Uint8Array {
  return encodeTyped(message as unknown as never, schemaId.Message, registry);
}

/** A reusable compat decode program for the `Message` envelope, yielding the
 * ergonomic `{ tag, value }` shape (`decodeTyped`). */
export type MessageDecoder = (bytes: Uint8Array) => Message;

/**
 * Build a decoder for incoming `Message`s. `peerSchemaBytes` is the peer's
 * envelope schema closure (phon self-describing schema bytes) from the handshake;
 * when absent, our own schema is the writer (schema-identical degenerate of the one
 * compat path).
 */
// r[impl conduit.typeplan]
export function buildMessageDecoder(peerSchemaBytes?: Uint8Array): MessageDecoder {
  if (!peerSchemaBytes || peerSchemaBytes.length === 0) {
    return (bytes) =>
      decodeTyped(bytes, schemaId.Message, schemaId.Message, registry) as unknown as Message;
  }
  const { root, reg } = mergeWriterSchemas(peerSchemaBytes, registry);
  return (bytes) => decodeTyped(bytes, root, schemaId.Message, reg) as unknown as Message;
}

/** Decode a `Message` with a prebuilt decoder. */
export function decodeMessageWith(decoder: MessageDecoder, bytes: Uint8Array): Message {
  return decoder(bytes);
}

/** Decode a `Message` against our own (same-version) envelope schema. */
export function decodeMessage(bytes: Uint8Array): Message {
  return decodeTyped(bytes, schemaId.Message, schemaId.Message, registry) as unknown as Message;
}

/**
 * Parse a phon schema binding (`u64 primaryRoot + u32 count + [u32 len + schema]*`,
 * optionally followed by auxiliary roots). Mirrors vox-phon's schema binding
 * framing.
 */
export interface AuxiliaryRoot {
  role: string;
  root: bigint;
}

// r[impl schema.principles.self-describing]
// r[impl schema.format.binding-roots]
export function parseSchemaClosure(bytes: Uint8Array): {
  root: bigint;
  schemas: Schema[];
  auxiliaryRoots: AuxiliaryRoot[];
} {
  const dv = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  let off = 0;
  const root = dv.getBigUint64(off, true);
  off += 8;
  const count = dv.getUint32(off, true);
  off += 4;
  const schemas: Schema[] = [];
  for (let i = 0; i < count; i++) {
    const len = dv.getUint32(off, true);
    off += 4;
    const slice = bytes.subarray(off, off + len);
    off += len;
    schemas.push(schemaFromBytes(slice));
  }
  const auxiliaryRoots: AuxiliaryRoot[] = [];
  if (off < bytes.byteLength) {
    const auxCount = dv.getUint32(off, true);
    off += 4;
    const decoder = new TextDecoder();
    for (let i = 0; i < auxCount; i++) {
      const roleLen = dv.getUint32(off, true);
      off += 4;
      const role = decoder.decode(bytes.subarray(off, off + roleLen));
      off += roleLen;
      const auxRoot = dv.getBigUint64(off, true);
      off += 8;
      auxiliaryRoots.push({ role, root: auxRoot });
    }
  }
  if (off !== bytes.byteLength) {
    throw new Error(`schema binding has ${bytes.byteLength - off} trailing bytes`);
  }
  return { root, schemas, auxiliaryRoots };
}

function mergeWriterSchemas(
  peerSchemaBytes: Uint8Array,
  local: Registry,
): { root: bigint; reg: Registry } {
  const { root, schemas } = parseSchemaClosure(peerSchemaBytes);
  return { root, reg: local.with(schemas) };
}
